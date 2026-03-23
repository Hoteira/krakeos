import sys, struct
with open(sys.argv[1], 'rb') as f:
    data = f.read()
num_tables = struct.unpack_from('>H', data, 4)[0]
for i in range(num_tables):
    offset = 12 + i * 16
    tag = data[offset:offset+4].decode('ascii', errors='ignore')
    c, o, l = struct.unpack_from('>III', data, offset+4)
    print(f"Table {tag}: offset {o} (0x{o:x}), length {l} (0x{l:x})")
