import re
from pathlib import Path
root = Path(r"D:/Work Development/Repos/O-DRAFT/crates/engine/src/cmd")
rows = []
pat = re.compile(r'CommandSpec::new\("([^"]+)"[\s\S]*?\.menu\(\&\[([^\]]+)\]\)', re.MULTILINE)
for p in sorted(root.rglob("*.rs")):
    t = p.read_text(encoding="utf-8")
    for m in pat.finditer(t):
        parts = [x.strip().strip('"') for x in m.group(2).split(",")]
        rows.append((m.group(1), " > ".join(parts), p.name))
rows = sorted(set(rows), key=lambda r: (r[1].lower(), r[0]))
Path(r"D:/Work Development/Repos/O-DRAFT/docs/qa/_menu_cmds_temp.tsv").write_text("\n".join(f"{a}\t{b}\t{c}" for a, b, c in rows), encoding="utf-8")
print(len(rows))
