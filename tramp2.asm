[bits 16]
org 0x8000
cli
cld
xor ax, ax
mov ds, ax
mov es, ax
mov ss, ax
lgdt [0x804e]
mov eax, cr0
or eax, 1
mov cr0, eax
jmp dword 0x08:protected_mode

[bits 32]
protected_mode:
mov ax, 0x10
mov ds, ax
mov es, ax
mov ss, ax
mov eax, cr4
or eax, 0x20
mov cr4, eax
mov eax, [0x8060]
mov cr3, eax
mov ecx, 0xC0000080
rdmsr
or eax, 0x100
wrmsr
mov eax, cr0
or eax, 0x80000000
mov cr0, eax
lgdt [0x8058]
jmp 0x18:long_mode

[bits 64]
long_mode:
mov ax, 0x00
mov ds, ax
mov es, ax
mov ss, ax
mov fs, ax
mov gs, ax
mov rsp, [0x8068]
mov rax, [0x8070]
call rax
jmp $
