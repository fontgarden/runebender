# Adds Runebender to Steam on the Steam Deck as a Non-Steam Game, with art.
#
#   steam -shutdown            (Steam must be closed while this runs)
#   python3 steamdeck/install_steam.py
#
# It writes the shortcut into Steam's shortcuts.vdf and copies the library art
# (in steamdeck/art) into Steam's grid folder. Run it again to update the art.
import glob
import os
import shutil
import struct
import subprocess
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
GAME = os.path.normpath(os.path.join(HERE, ".."))
NAME = "Runebender"
EXE = os.path.join(HERE, "run.sh")
ART = os.path.join(HERE, "art")


def read_vdf(data):
    """Reads Steam's binary vdf into nested dicts. Only what shortcuts.vdf uses."""
    pos = 0

    def cstr():
        nonlocal pos
        end = data.index(b"\0", pos)
        s = data[pos:end].decode("utf-8", "replace")
        pos = end + 1
        return s

    def node():
        nonlocal pos
        out = {}
        while True:
            kind = data[pos]
            pos += 1
            if kind == 8:
                return out
            key = cstr()
            if kind == 0:
                out[key] = node()
            elif kind == 1:
                out[key] = cstr()
            elif kind == 2:
                out[key] = struct.unpack_from("<I", data, pos)[0]
                pos += 4
            else:
                raise ValueError(f"unknown vdf type {kind}")

    return node()


def write_vdf(d):
    out = bytearray()
    for k, v in d.items():
        if isinstance(v, dict):
            out += b"\0" + k.encode() + b"\0" + write_vdf(v) + b"\x08"
        elif isinstance(v, int):
            out += b"\x02" + k.encode() + b"\0" + struct.pack("<I", v)
        else:
            out += b"\x01" + k.encode() + b"\0" + v.encode() + b"\0"
    return bytes(out)


def main():
    if subprocess.run(["pgrep", "-x", "steam"], capture_output=True).returncode == 0:
        sys.exit("Steam is running. Close it first:  steam -shutdown")
    users = glob.glob(os.path.expanduser("~/.steam/steam/userdata/*/config"))
    users = [u for u in users if os.path.basename(os.path.dirname(u)) != "0"]
    if not users:
        sys.exit("no Steam user folder found")
    config = users[0]
    path = os.path.join(config, "shortcuts.vdf")
    root = read_vdf(open(path, "rb").read()) if os.path.exists(path) else {"shortcuts": {}}
    shortcuts = root.setdefault("shortcuts", {})

    # Steam numbers Non-Steam games this way, and names the art files after it
    appid = (zlib.crc32((EXE + NAME).encode()) | 0x80000000) & 0xFFFFFFFF
    entry = {
        "appid": appid,
        "AppName": NAME,
        "Exe": f'"{EXE}"',
        "StartDir": f'"{GAME}"',
        "icon": os.path.join(ART, "icon.png"),
        "ShortcutPath": "",
        "LaunchOptions": "",
        "IsHidden": 0,
        "AllowDesktopConfig": 1,
        "AllowOverlay": 1,
        "OpenVR": 0,
        "Devkit": 0,
        "DevkitGameID": "",
        "DevkitOverrideAppID": 0,
        "LastPlayTime": 0,
        "FlatpakAppID": "",
        "tags": {},
    }
    for k, v in list(shortcuts.items()):
        if v.get("AppName") == NAME:
            entry["LastPlayTime"] = v.get("LastPlayTime", 0)
            shortcuts[k] = entry
            break
    else:
        shortcuts[str(len(shortcuts))] = entry
    if os.path.exists(path):
        shutil.copy(path, path + ".bak")
    open(path, "wb").write(write_vdf(root) + b"\x08")

    grid = os.path.join(config, "grid")
    os.makedirs(grid, exist_ok=True)
    for src, dst in (("portrait.png", f"{appid}p.png"), ("landscape.png", f"{appid}.png"),
                     ("hero.png", f"{appid}_hero.png")):
        shutil.copy(os.path.join(ART, src), os.path.join(grid, dst))
    print(f"added '{NAME}' as app {appid} for Steam user {os.path.basename(os.path.dirname(config))}")
    print("start Steam again, and look under Non-Steam in the library")


main()
