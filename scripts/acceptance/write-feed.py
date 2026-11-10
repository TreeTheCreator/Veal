# python write-feed.py <feed-dir> <version>
# Writes latest.json for the local update feed, from the signed test installer
# "Veal Test_<version>_x64-setup.exe" and its .sig in <feed-dir>. Serve it with
#   python -m http.server 8765 --bind 127.0.0.1 --directory <feed-dir>
import datetime, json, sys

feed, v = sys.argv[1], sys.argv[2]
inst = f"Veal Test_{v}_x64-setup.exe"
sig = open(f"{feed}/{inst}.sig", encoding="utf-8").read().strip()
latest = {
    "version": v,
    "notes": "test update",
    "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
    "platforms": {"windows-x86_64": {"signature": sig, "url": f"http://127.0.0.1:8765/Veal%20Test_{v}_x64-setup.exe"}},
}
open(f"{feed}/latest.json", "w").write(json.dumps(latest, indent=2))
print(f"wrote latest.json for {v}")
