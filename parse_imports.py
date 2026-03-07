import sys

def parse_varuint32(f):
    res = 0
    shift = 0
    while True:
        b = f.read(1)[0]
        res |= (b & 0x7f) << shift
        if not (b & 0x80):
            break
        shift += 7
    return res

def parse_string(f):
    l = parse_varuint32(f)
    return f.read(l).decode('utf-8')

def main():
    with open("tree/sys/bin/init.wasm", "rb") as f:
        magic = f.read(4)
        version = f.read(4)
        
        types = []
        
        while True:
            sec_id = f.read(1)
            if not sec_id:
                break
            sec_id = sec_id[0]
            sec_len = parse_varuint32(f)
            
            if sec_id == 1: # Type section
                num_types = parse_varuint32(f)
                for _ in range(num_types):
                    form = f.read(1)[0]
                    num_params = parse_varuint32(f)
                    params = [f.read(1)[0] for _ in range(num_params)]
                    num_results = parse_varuint32(f)
                    results = [f.read(1)[0] for _ in range(num_results)]
                    types.append((params, results))
                    
            elif sec_id == 2: # Import section
                num_imports = parse_varuint32(f)
                for _ in range(num_imports):
                    mod = parse_string(f)
                    name = parse_string(f)
                    kind = f.read(1)[0]
                    if kind == 0:
                        type_idx = parse_varuint32(f)
                        if mod == "env" and name == "__wasi_init_tp":
                            print(f"__wasi_init_tp type index: {type_idx}")
                            if type_idx < len(types):
                                p, r = types[type_idx]
                                print(f"Params: {p}, Results: {r}")
                    elif kind == 1:
                        parse_varuint32(f)
                        parse_varuint32(f)
                        # might have max
                    elif kind == 2:
                        parse_varuint32(f)
                        parse_varuint32(f)
                    elif kind == 3:
                        parse_varuint32(f)
                        f.read(1)
            else:
                f.read(sec_len)

if __name__ == "__main__":
    main()
