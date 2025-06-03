section .data
    wSystemCall         dq 0
    qSyscallInsAdress   dq 0

section .text
    default rel         ; enable RIP-relative addressing

    global SetSSn
    global RunSyscall

SetSSn:
    mov eax, ecx
    mov [rel wSystemCall], rax
    mov r8, rdx
    mov [rel qSyscallInsAdress], r8
    ret

RunSyscall:
    mov rax, rcx
    mov r10, rax
    mov eax, dword [rel wSystemCall]
    jmp qword [rel qSyscallInsAdress]