import sys

def parse_varuint32(f):
    res = 0
    shift = 0
    while True:
        b = f.read(1)
        if not b:
            return None
        b = b[0]
        res |= (b & 0x7f) << shift
        if not (b & 0x80):
            break
        shift += 7
    return res

def parse_string(f):
    l = parse_varuint32(f)
    if l is None: return None
    return f.read(l).decode('utf-8')

def main():
    with open("tree/sys/bin/init.wasm", "rb") as f:
        magic = f.read(4)
        version = f.read(4)
        
        while True:
            sec_id_b = f.read(1)
            if not sec_id_b:
                break
            sec_id = sec_id_b[0]
            sec_len = parse_varuint32(f)
            
            if sec_id == 7: # Export section
                num_exports = parse_varuint32(f)
                for _ in range(num_exports):
                    name = parse_string(f)
                    kind = f.read(1)[0]
                    idx = parse_varuint32(f)
                    print(f"Export: {name} (kind {kind}, idx {idx})")
            else:
                f.read(sec_len)

if __name__ == "__main__":
    main()
