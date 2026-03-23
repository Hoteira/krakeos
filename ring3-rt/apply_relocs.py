#!/usr/bin/env python3
"""Apply R_X86_64_RELATIVE relocations from an ELF file to a raw binary."""
import struct
import sys

def apply_relocs(elf_path, bin_path):
    with open(elf_path, 'rb') as f:
        elf = f.read()
    with open(bin_path, 'rb') as f:
        binary = bytearray(f.read())

    # Parse ELF header
    e_shoff = struct.unpack_from('<Q', elf, 40)[0]
    e_shentsize = struct.unpack_from('<H', elf, 58)[0]
    e_shnum = struct.unpack_from('<H', elf, 60)[0]

    # Find .rela.dyn section
    for i in range(e_shnum):
        sh_offset_in_header = e_shoff + i * e_shentsize
        sh_type = struct.unpack_from('<I', elf, sh_offset_in_header + 4)[0]
        if sh_type == 4:  # SHT_RELA
            sh_offset = struct.unpack_from('<Q', elf, sh_offset_in_header + 24)[0]
            sh_size = struct.unpack_from('<Q', elf, sh_offset_in_header + 32)[0]
            sh_entsize = struct.unpack_from('<Q', elf, sh_offset_in_header + 56)[0]
            num_entries = sh_size // sh_entsize

            count = 0
            for j in range(num_entries):
                entry_off = sh_offset + j * sh_entsize
                r_offset = struct.unpack_from('<Q', elf, entry_off)[0]
                r_info = struct.unpack_from('<Q', elf, entry_off + 8)[0]
                r_addend = struct.unpack_from('<q', elf, entry_off + 16)[0]
                r_type = r_info & 0xFFFFFFFF

                if r_type == 8:  # R_X86_64_RELATIVE
                    if r_offset + 8 <= len(binary):
                        struct.pack_into('<Q', binary, r_offset, r_addend)
                        count += 1

            print(f"Applied {count} R_X86_64_RELATIVE relocations", file=sys.stderr)
            break

    with open(bin_path, 'wb') as f:
        f.write(binary)

if __name__ == '__main__':
    apply_relocs(sys.argv[1], sys.argv[2])
