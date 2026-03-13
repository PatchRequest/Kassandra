# Kassandra - Rust Mythic Agent

**Kassandra** is a custom Mythic C2 agent written in **Rust**, containerized via a **Python-based builder**. It is currently in development and includes several advanced post-exploitation and pivoting features. 


> This public release of the agent does not include all implemented obfuscation and defense evasion techniques. Several components such as advanced in-memory obfuscation, indirect syscalls, and full transport stealth—have been stripped or simplified intentionally to limit abuse and make replication harder for script kiddies. The full version remains private for controlled red team use.

<p align="center">
  <img src="./KassandraLogo.png" width="400" />
</p>

## Installation

From the Mythic install directory, use the following command to install Kassandra:
```bash
cd /path/to/Mythic
sudo ./mythic-cli install github https://github.com/PatchRequest/Kassandra
```

Or from a local folder:
```bash
sudo ./mythic-cli install folder /path/to/Kassandra
```

## ⚙ Features

* **Syscall Evasion:**

  * `Hell's Hall` for stealthy syscall resolution

* **Security Context Control:**

  * Modify the **Security Descriptor** of the current process to restrict/allow interaction

* **Filesystem Ops:**

  * Upload / Download files
  * Enumerate directories and file attributes

* **Process Management:**

  * List running processes

* **In-Memory Execution:**

  * Execute **.NET assemblies** in memory
  * Load and run **Beacon object files (.boF)** in memory

* **C2 Transports:**

  * **HTTP** — Standard Mythic HTTP C2 profile
  * **[S3 Storage](https://github.com/Yeeb1/awss3)** — S3-based C2 transport with AWS SigV4 signing, bootstrap registration for per-execution IAM credential isolation, and AES-256-CBC encryption with HMAC-SHA256 (EKE)
  * **[Tailscale](https://github.com/Yeeb1/mythic_tailscale)** — Embedded Tailscale/Headscale C2 transport via Go FFI, supporting HTTP and raw TCP protocols over WireGuard tunnels with optional DNS-over-HTTPS

* **Proxy & Pivot:**

  * Start a **socket proxy** tunnel via the teamserver
  * Use the agent as a **pivot endpoint** for other agents

* **Execution:**

  * Run arbitrary **PowerShell commands**

* **Reconnaissance:**

  * Take screenshots (GDI-based capture, PNG-encoded)


## 🔧 Notes

* **Not yet complete:**

  * Full encryption of transport and task responses


## 📁 Structure

```
/agent_code/kassandra/
├── src/
│   ├── main.rs
│   ├── transport/
│   ├── tasks/
│   └── ...
├── build.rs
└── Cargo.toml
```

## 🚧 Disclaimer

This project is for **educational and red teaming** purposes only. Do not use without proper authorization.

---

Thanks to [@Yeeb1](https://github.com/Yeeb1) for contributing the [awss3](https://github.com/Yeeb1/awss3) S3 Storage C2 profile integration, the [Tailscale C2 transport](https://github.com/Yeeb1/mythic_tailscale), and agent improvements

Thanks to MalDevAcademy for their high-quality malware development training, VX-Underground for curating an essential archive of offensive research, and also to @ZkClown and Ze_Asimovitch for their continuous inspiration and contributions to the red teaming community
