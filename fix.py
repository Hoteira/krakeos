import re

with open("std/src/wasm/aot/compiler.rs", "r") as f:
    code = f.read()

# Pattern to find the sequence:
# self.emitter.modrm(1, Reg/XmmReg as u8, Reg::RCX as u8);
# self.emitter.emit_u8(memarg.offset as u8);
# And replace with:
# if memarg.offset <= 127 {
#     self.emitter.modrm(1, ..., Reg::RCX as u8);
#     self.emitter.emit_u8(memarg.offset as u8);
# } else {
#     self.emitter.modrm(2, ..., Reg::RCX as u8);
#     self.emitter.emit_u32(memarg.offset);
# }

def repl(m):
    reg = m.group(1)
    return f"""if memarg.offset <= 127 {{
            self.emitter.modrm(1, {reg}, Reg::RCX as u8);
            self.emitter.emit_u8(memarg.offset as u8);
        }} else {{
            self.emitter.modrm(2, {reg}, Reg::RCX as u8);
            self.emitter.emit_u32(memarg.offset);
        }}"""

new_code = re.sub(
    r"self\.emitter\.modrm\(1,\s*(.+?),\s*Reg::RCX as u8\);\s*self\.emitter\.emit_u8\(memarg\.offset as u8\);",
    repl,
    code
)

with open("std/src/wasm/aot/compiler.rs", "w") as f:
    f.write(new_code)
