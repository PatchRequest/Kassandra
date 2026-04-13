package main

/*
#include <stdlib.h>
#include <string.h>
*/
import "C"
import (
	"bytes"
	"context"
	"encoding/binary"
	"io"
	"net"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"
	"unsafe"

	"tailscale.com/ipn/store/mem"
	"tailscale.com/net/dnscache"
	"tailscale.com/tsnet"
)

var (
	mu       sync.Mutex
	tcpMu    sync.Mutex // separate mutex for TCP send/receive serialization
	tsServer *tsnet.Server
	client   *http.Client
	tcpConn  net.Conn
	initDone bool
	tmpDir   string
)

// ts_set_doh configures selective DNS-over-HTTPS for Tailscale/Headscale domains only.
// All other DNS queries use the system resolver so local/internal domains still work.
//
//export ts_set_doh
func ts_set_doh(doh_url *C.char, control_url *C.char) {
	goURL := C.GoString(doh_url)
	if goURL == "" {
		return
	}
	goControlURL := C.GoString(control_url)

	// Build the set of domains to resolve via DoH
	dohDomains := []string{".tailscale.com"}
	if goControlURL != "" {
		if h := extractHost(goControlURL); h != "" && !strings.HasSuffix(h, ".tailscale.com") {
			dohDomains = append(dohDomains, h)
		}
	}

	dohTransport := &http.Transport{
		DialContext: (&net.Dialer{Timeout: 5 * time.Second}).DialContext,
	}
	dohClient := &http.Client{Timeout: 5 * time.Second, Transport: dohTransport}

	dohResolver := &net.Resolver{
		PreferGo: true,
		Dial: func(ctx context.Context, network, address string) (net.Conn, error) {
			return &dohConn{ctx: ctx, dohURL: goURL, dohClient: dohClient}, nil
		},
	}

	shouldDoH := func(host string) bool {
		lower := strings.ToLower(host)
		for _, d := range dohDomains {
			if strings.HasSuffix(lower, d) || lower == strings.TrimPrefix(d, ".") {
				return true
			}
		}
		return false
	}

	// Snapshot the system resolver before overriding
	systemResolver := *net.DefaultResolver

	// Override net.DefaultResolver
	net.DefaultResolver = dohResolver

	// Patch the dnscache singleton — this is the actual resolver used by tsnet's
	// controlhttp (controlplane.tailscale.com) and logpolicy (log.tailscale.com).
	// The singleton is created at package init with &net.Resolver{} (nil Dial),
	// which bypasses net.DefaultResolver entirely.
	dnscache.Get().Forward = dohResolver

	resolveHost := func(ctx context.Context, host string) (string, error) {
		if shouldDoH(host) {
			ips, err := dohResolver.LookupHost(ctx, host)
			if err != nil {
				return "", err
			}
			return ips[0], nil
		}
		ips, err := systemResolver.LookupHost(ctx, host)
		if err != nil {
			return "", err
		}
		return ips[0], nil
	}

	// Patch http.DefaultTransport — only resolve Tailscale/Headscale domains via DoH
	baseTransport, ok := http.DefaultTransport.(*http.Transport)
	if !ok {
		baseTransport = &http.Transport{}
	}
	patched := baseTransport.Clone()
	patched.DialContext = func(ctx context.Context, network, addr string) (net.Conn, error) {
		host, port, err := net.SplitHostPort(addr)
		if err != nil {
			return (&net.Dialer{Timeout: 10 * time.Second}).DialContext(ctx, network, addr)
		}
		if net.ParseIP(host) != nil {
			return (&net.Dialer{Timeout: 10 * time.Second}).DialContext(ctx, network, addr)
		}
		ip, err := resolveHost(ctx, host)
		if err != nil {
			return (&net.Dialer{Timeout: 10 * time.Second}).DialContext(ctx, network, addr)
		}
		return (&net.Dialer{Timeout: 10 * time.Second}).DialContext(ctx, network, net.JoinHostPort(ip, port))
	}

	http.DefaultTransport = patched
}

func extractHost(rawURL string) string {
	u := rawURL
	if idx := strings.Index(u, "://"); idx >= 0 {
		u = u[idx+3:]
	}
	if idx := strings.Index(u, "/"); idx >= 0 {
		u = u[:idx]
	}
	if host, _, err := net.SplitHostPort(u); err == nil {
		return host
	}
	return u
}

// dohConn implements net.Conn to intercept DNS queries and forward them via DoH.
type dohConn struct {
	ctx       context.Context
	dohURL    string
	dohClient *http.Client
	resp      []byte
	readPos   int
}

func (c *dohConn) Write(b []byte) (int, error) {
	req, err := http.NewRequestWithContext(c.ctx, "POST", c.dohURL, bytes.NewReader(b))
	if err != nil {
		return 0, err
	}
	req.Header.Set("Content-Type", "application/dns-message")
	req.Header.Set("Accept", "application/dns-message")
	resp, err := c.dohClient.Do(req)
	if err != nil {
		return 0, err
	}
	defer resp.Body.Close()
	c.resp, err = io.ReadAll(resp.Body)
	if err != nil {
		return 0, err
	}
	c.readPos = 0
	return len(b), nil
}

func (c *dohConn) Read(b []byte) (int, error) {
	if c.readPos >= len(c.resp) {
		return 0, io.EOF
	}
	n := copy(b, c.resp[c.readPos:])
	c.readPos += n
	return n, nil
}

func (c *dohConn) Close() error                       { return nil }
func (c *dohConn) LocalAddr() net.Addr                { return nil }
func (c *dohConn) RemoteAddr() net.Addr               { return nil }
func (c *dohConn) SetDeadline(t time.Time) error      { return nil }
func (c *dohConn) SetReadDeadline(t time.Time) error  { return nil }
func (c *dohConn) SetWriteDeadline(t time.Time) error { return nil }

//export ts_init
func ts_init(auth_key *C.char, control_url *C.char, hostname *C.char) C.int {
	mu.Lock()
	defer mu.Unlock()
	if initDone {
		return 0
	}

	goAuthKey := C.GoString(auth_key)
	goControlURL := C.GoString(control_url)
	goHostname := C.GoString(hostname)

	// Create a temp dir for tsnet internals — cleaned up on ts_close()
	var err error
	tmpDir, err = os.MkdirTemp("", "ts-")
	if err != nil {
		return -1
	}

	tsServer = &tsnet.Server{
		Dir:       tmpDir,
		Hostname:  goHostname,
		AuthKey:   goAuthKey,
		Ephemeral: true,
		Store:     new(mem.Store),
		Logf:      func(string, ...any) {}, // suppress logs
	}
	if goControlURL != "" {
		tsServer.ControlURL = goControlURL
	}

	ctx, cancel := context.WithTimeout(context.Background(), 120*time.Second)
	defer cancel()

	_, err = tsServer.Up(ctx)
	if err != nil {
		return -1
	}

	client = tsServer.HTTPClient()
	initDone = true
	return 0
}

//export ts_http_post
func ts_http_post(url *C.char, body unsafe.Pointer, body_len C.int, resp_buf unsafe.Pointer, resp_buf_len C.int) C.int {
	mu.Lock()
	c := client
	mu.Unlock()

	if c == nil {
		return -1
	}

	goURL := C.GoString(url)
	goBody := C.GoBytes(body, body_len)

	req, err := http.NewRequest("POST", goURL, bytes.NewReader(goBody))
	if err != nil {
		return -1
	}
	req.Header.Set("Content-Type", "application/octet-stream")

	resp, err := c.Do(req)
	if err != nil {
		return -1
	}
	defer resp.Body.Close()

	respData, err := io.ReadAll(resp.Body)
	if err != nil {
		return -1
	}

	if len(respData) > int(resp_buf_len) {
		return -2 // buffer too small
	}

	if len(respData) > 0 {
		C.memcpy(resp_buf, unsafe.Pointer(&respData[0]), C.size_t(len(respData)))
	}
	return C.int(len(respData))
}

//export ts_tcp_connect
func ts_tcp_connect(host *C.char, port *C.char) C.int {
	mu.Lock()
	defer mu.Unlock()

	if tsServer == nil || !initDone {
		return -1
	}

	goHost := C.GoString(host)
	goPort := C.GoString(port)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	conn, err := tsServer.Dial(ctx, "tcp", net.JoinHostPort(goHost, goPort))
	if err != nil {
		return -1
	}

	if tcpConn != nil {
		tcpConn.Close()
	}
	tcpConn = conn
	return 0
}

//export ts_tcp_send
func ts_tcp_send(body unsafe.Pointer, body_len C.int, resp_buf unsafe.Pointer, resp_buf_len C.int) C.int {
	// Hold tcpMu for the entire send+receive to prevent interleaved framing
	// from concurrent callers (e.g. SOCKS proxy + tasking).
	tcpMu.Lock()
	defer tcpMu.Unlock()

	mu.Lock()
	conn := tcpConn
	mu.Unlock()

	if conn == nil {
		return -1
	}

	goBody := C.GoBytes(body, body_len)

	// Write length-prefixed message: [4-byte big-endian length][payload]
	if err := binary.Write(conn, binary.BigEndian, uint32(len(goBody))); err != nil {
		return -3 // write error, needs reconnect
	}
	if _, err := conn.Write(goBody); err != nil {
		return -3
	}

	// Read response length
	var respLen uint32
	if err := binary.Read(conn, binary.BigEndian, &respLen); err != nil {
		return -3
	}
	if respLen == 0 {
		return 0
	}
	if respLen > uint32(resp_buf_len) {
		return -2 // buffer too small
	}

	// Read response payload
	respData := make([]byte, respLen)
	if _, err := io.ReadFull(conn, respData); err != nil {
		return -3
	}

	C.memcpy(resp_buf, unsafe.Pointer(&respData[0]), C.size_t(len(respData)))
	return C.int(len(respData))
}

//export ts_close
func ts_close() {
	mu.Lock()
	defer mu.Unlock()
	if tcpConn != nil {
		tcpConn.Close()
		tcpConn = nil
	}
	if tsServer != nil {
		tsServer.Close()
		tsServer = nil
		client = nil
		initDone = false
	}
	if tmpDir != "" {
		os.RemoveAll(tmpDir)
		tmpDir = ""
	}
}

func main() {}
