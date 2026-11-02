"""Generates the ODM suite as a figcraft-cli script, then builds and exports it.

Run: python design/odm-suite/build.py
Writes odm-suite.figcraft and png/*.png next to this file.
"""
import json
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).parent
CLI = r"C:\Program Files\ArtCraft\FigCraft\figcraft-cli.exe"
FONT = "Inter"

# Same names and values as app/src/app.css, so the tokens map straight onto the app.
TOKENS = {
    "bg": "#000000", "surface": "#1B1B1B", "surface-2": "#2F2F2F", "border": "#3A3A3A",
    "text": "#FFFFFF", "muted": "#A0A0A0", "accent": "#FFA31A", "accent-light": "#FFB84D",
    "on-accent": "#000000", "error": "#FF6B6B", "error-tint": "#291111", "success": "#34D399",
    "success-tint": "#082218", "warn": "#A78BFA", "warn-tint": "#211C32", "info": "#60A5FA",
    "idle": "#6B7280", "selected": "#291A04", "selected-hover": "#382406",
}

steps = []
counter = [0]


def cmd(command, params, name=None):
    step = {"command": command, "params": params}
    if name:
        step["as"] = name
    steps.append(step)


def fresh(prefix="n"):
    counter[0] += 1
    return f"{prefix}{counter[0]}"


def ref(name):
    return {"$ref": name}


def paint(value):
    return {"$var": value} if value in TOKENS else value


def box(parent=None, name="Frame", w=None, h=None, dir=None, gap=None, pad=None, fill="none", r=None,
        stroke=None, main=None, cross=None, sw=None, sh=None, x=None, y=None, clip=None, absolute=False,
        props=None):
    me = fresh("f")
    p = {"fill": paint(fill)}
    if dir:
        p["layout"] = dir
        p["sizingH"] = sw or ("hug" if w is None else "fixed")
        p["sizingV"] = sh or ("hug" if h is None else "fixed")
    else:
        if sw:
            p["sizingH"] = sw
        if sh:
            p["sizingV"] = sh
    if gap is not None:
        p["gap"] = gap
    if pad is not None:
        p["padding"] = pad
    if r is not None:
        p["cornerRadius"] = r
    if stroke:
        p["stroke"] = paint(stroke)
        p["strokeWeight"] = 1
        p["strokeAlign"] = "inside"
    if main:
        p["mainAlign"] = main
    if cross:
        p["crossAlign"] = cross
    if clip is not None:
        p["clip"] = clip
    if absolute:
        p["absolute"] = True
    if props:
        p.update(props)
    q = {"type": "frame", "name": name, "props": p, "select": False}
    if parent:
        q["parent"] = ref(parent)
    for key, value in (("width", w), ("height", h), ("x", x), ("y", y)):
        if value is not None:
            q[key] = value
    cmd("node.create", q, me)
    return me


def text(parent, s, size=13, weight="Regular", color="text", w=None, fill_w=False, align=None, lh=None, tnum=False,
         x=None, y=None, truncate=False, name=None):
    me = fresh("t")
    p = {"fontFamily": FONT, "fontStyle": weight, "fontSize": size, "fill": paint(color)}
    if lh:
        p["lineHeight"] = lh
    if align:
        p["textAlign"] = align
    if tnum:
        p["features"] = {"tnum": True}
    if fill_w:
        p["sizingH"] = "fill"
        p["textAutoResize"] = "truncate" if truncate else "height"
    elif w:
        p["textAutoResize"] = "truncate" if truncate else "height"
    q = {"type": "text", "parent": ref(parent), "text": s, "props": p, "select": False, "name": name or s[:32]}
    if w:
        q["width"] = w
    if x is not None:
        q["x"] = x
    if y is not None:
        q["y"] = y
    cmd("node.create", q, me)
    return me


def spacer(parent):
    return box(parent, "Spacer", h=1, dir="horizontal", sw="fill")


# Icons are drawn on a 24 grid with M, L, C and Z only, so scaling is a plain number multiply.
def circle(cx, cy, r):
    k = 0.5523 * r
    return (f"M{cx - r} {cy}C{cx - r} {cy - k} {cx - k} {cy - r} {cx} {cy - r}"
            f"C{cx + k} {cy - r} {cx + r} {cy - k} {cx + r} {cy}"
            f"C{cx + r} {cy + k} {cx + k} {cy + r} {cx} {cy + r}"
            f"C{cx - k} {cy + r} {cx - r} {cy + k} {cx - r} {cy}Z")


def rrect(x, y, w, h, r):
    k = r * 0.4477
    return (f"M{x + r} {y}L{x + w - r} {y}C{x + w - k} {y} {x + w} {y + k} {x + w} {y + r}"
            f"L{x + w} {y + h - r}C{x + w} {y + h - k} {x + w - k} {y + h} {x + w - r} {y + h}"
            f"L{x + r} {y + h}C{x + k} {y + h} {x} {y + h - k} {x} {y + h - r}"
            f"L{x} {y + r}C{x} {y + k} {x + k} {y} {x + r} {y}Z")


ICONS = {
    "pause": "M8 5L8 19M16 5L16 19",
    "play": "M7 4L19 12L7 20Z",
    "x": "M6 6L18 18M18 6L6 18",
    "check": "M5 12.5L10 17.5L19 7",
    "more": circle(5, 12, 0.9) + circle(12, 12, 0.9) + circle(19, 12, 0.9),
    "folder": "M3 7C3 5.9 3.9 5 5 5L9 5L11 8L19 8C20.1 8 21 8.9 21 10L21 18C21 19.1 20.1 20 19 20L5 20C3.9 20 3 19.1 3 18Z",
    "sliders": "M4 7L13 7M17 7L20 7M4 17L7 17M11 17L20 17" + circle(15, 7, 2) + circle(9, 17, 2),
    "chevron": "M6 9L12 15L18 9",
    "chevron-right": "M9 6L15 12L9 18",
    "download": "M12 4L12 15M7 10.5L12 15.5L17 10.5M5 20L19 20",
    "search": circle(11, 11, 7) + "M16.5 16.5L21 21",
    "video": rrect(3, 5, 18, 14, 2) + "M10 9L15 12L10 15Z",
    "archive": rrect(3, 4, 18, 5, 1) + "M5 9L5 20L19 20L19 9M10 13L14 13",
    "music": "M9 18L9 6L20 4L20 16" + circle(6.5, 18, 2.5) + circle(17.5, 16, 2.5),
    "image": rrect(3, 4, 18, 16, 2) + circle(8.5, 9.5, 1.5) + "M21 16L16 11L6 20",
    "doc": "M6 3L14 3L19 8L19 21L6 21ZM14 3L14 8L19 8",
    "program": "M12 3L20 7.5L20 16.5L12 21L4 16.5L4 7.5ZM4 7.5L12 12L20 7.5M12 12L12 21",
    "magnet": "M5 4L5 12C5 15.9 8.1 19 12 19C15.9 19 19 15.9 19 12L19 4L15 4L15 12C15 13.7 13.7 15 12 15C10.3 15 9 13.7 9 12L9 4ZM5 8L9 8M15 8L19 8",
    "alert": "M12 4L21.5 20L2.5 20ZM12 10L12 14.5M12 17.2L12 17.3",
    "refresh": "M20 11C19.5 7 16 4 12 4C8.6 4 5.6 6 4.5 9M4 13C4.5 17 8 20 12 20C15.4 20 18.4 18 19.5 15M4 4L4 9L9 9M20 20L20 15L15 15",
    "globe": circle(12, 12, 9) + "M3 12L21 12M12 3C15 6 15 18 12 21C9 18 9 6 12 3",
    "list": "M4 6L20 6M4 12L20 12M4 18L20 18",
    "cards": rrect(3, 4, 18, 7, 2) + rrect(3, 13, 18, 7, 2),
    "clock": circle(12, 12, 9) + "M12 7L12 12L15.5 14",
    "plus": "M12 5L12 19M5 12L19 12",
    "gauge": "M4 18C2.8 13 6 5 12 5C18 5 21.2 13 20 18M12 14L16 9",
    "add-many": "M4 7L20 7M4 12L13 12M4 17L13 17M18 13L18 21M14 17L22 17",
    "back": "M19 12L5 12M11 6L5 12L11 18",
    "grip": circle(9, 6, 0.9) + circle(15, 6, 0.9) + circle(9, 12, 0.9) + circle(15, 12, 0.9) + circle(9, 18, 0.9) + circle(15, 18, 0.9),
    "up": "M12 19L12 5M6 11L12 5L18 11",
    "down": "M12 5L12 19M6 13L12 19L18 13",
    "top": "M12 20L12 9M6 14L12 8L18 14M5 4L19 4",
    "trash": "M4 7L20 7M9 7L9 4L15 4L15 7M6 7L7 21L17 21L18 7",
    "copy": rrect(8, 8, 13, 13, 2) + "M16 8L16 5C16 3.9 15.1 3 14 3L5 3C3.9 3 3 3.9 3 5L3 14C3 15.1 3.9 16 5 16L8 16",
}


def scale_path(path, factor):
    return re.sub(r"-?\d+(?:\.\d+)?", lambda m: f"{float(m.group()) * factor:.3f}".rstrip("0").rstrip("."), path)


def icon(parent, name, size=18, color="text", weight=1.75, filled=False):
    holder = box(parent, f"Icon/{name}", w=size, h=size)
    q = {"type": "vector", "parent": ref(holder), "path": scale_path(ICONS[name], size / 24), "x": 0, "y": 0, "select": False,
         "name": name, "props": {"fill": paint(color) if filled else "none", "stroke": paint(color), "strokeWeight": weight,
                                 "strokeCap": "round", "strokeJoin": "round"}}
    cmd("node.create", q)
    return holder


BTN = {
    "primary": ("accent", None, "on-accent", "Bold"),
    "secondary": ("surface-2", "border", "text", "Semi Bold"),
    "ghost": ("none", None, "muted", "Semi Bold"),
    "danger": ("error-tint", "error", "error", "Semi Bold"),
}


def button(parent, label, kind="secondary", ic=None, h=32, size=13, full=False, name=None):
    fill, stroke, color, weight = BTN[kind]
    b = box(parent, name or f"Button/{kind}", h=h, dir="horizontal", gap=6, pad=[0, 12], fill=fill, stroke=stroke, r=7,
            main="center", cross="center", sw="fill" if full else None)
    if ic:
        icon(b, ic, 16, color)
    text(b, label, size, weight, color)
    return b


def icon_button(parent, ic, size=32, kind="secondary", color=None):
    fill, stroke, c, _ = BTN[kind]
    b = box(parent, f"IconButton/{ic}", w=size, h=size, dir="horizontal", fill=fill, stroke=stroke, r=7, main="center", cross="center")
    icon(b, ic, 16, color or c)
    return b


def pill(parent, label, color="muted", fill="surface-2"):
    p = box(parent, "Pill", h=22, dir="horizontal", pad=[0, 8], fill=fill, r=11, cross="center")
    text(p, label, 12, "Semi Bold", color, tnum=True)
    return p


def logo(parent, size=32):
    tile = box(parent, "ODM mark", w=size, h=size, fill="accent", r=size * 0.22)
    s = size / 1024
    path = scale_path("M512 232L512 620M344 468L512 636L680 468M288 772L736 772", s)
    cmd("node.create", {"type": "vector", "parent": ref(tile), "path": path, "x": 0, "y": 0, "select": False, "name": "arrow",
                        "props": {"fill": "none", "stroke": "#000000", "strokeWeight": 84 * s, "strokeCap": "round", "strokeJoin": "round"}})
    return tile


def bar(parent, width, pct, color="accent", h=4):
    track = box(parent, "Progress", w=width, h=h, fill="surface-2", r=h / 2, clip=True)
    box(track, "Value", w=max(1, width * pct / 100), h=h, fill=color, r=h / 2, x=0, y=0)
    return track


def header(root, W, value=None, focused=False):
    head = box(root, "Header", h=68, dir="horizontal", gap=12, pad=[0, 20], cross="center", sw="fill")
    logo(head)
    paste = box(head, "Paste bar", h=44, dir="horizontal", gap=10, pad=[0, 6, 0, 14], fill="surface", stroke="accent" if focused else "border",
                r=10, cross="center", sw="fill")
    icon(paste, "download", 18, "accent" if value else "muted")
    if value:
        text(paste, value, 14, "Medium", "text", fill_w=True, truncate=True, name="Pasted address")
    else:
        text(paste, "Paste a link or magnet, or drop a file here", 14, "Regular", "muted", fill_w=True, name="Paste hint")
        key = box(paste, "Key hint", h=24, dir="horizontal", pad=[0, 7], fill="surface-2", r=5, cross="center")
        text(key, "Ctrl V", 12, "Medium", "muted")
    button(head, "Add several", "secondary", "add-many", h=44)
    button(head, "Settings", "secondary", "sliders", h=44)
    return head


TABS = [("All", 6), ("Downloading", 2), ("Up next", 1), ("Finished", 2), ("Problems", 1)]


def tabs(root, active="All", detailed=False, counts=None):
    row = box(root, "Tabs", h=44, dir="horizontal", gap=4, pad=[0, 20], cross="center", sw="fill")
    for label, count in (counts or TABS):
        on = label == active
        t = box(row, f"Tab/{label}", h=32, dir="horizontal", gap=6, pad=[0, 10], fill="surface-2" if on else "none", r=7, cross="center")
        text(t, label, 13, "Semi Bold", "text" if on else "muted")
        problem = label == "Problems" and count
        text(t, str(count), 12, "Semi Bold", "error" if problem else "muted", tnum=True)
    spacer(row)
    type_filter = box(row, "Type filter", h=32, dir="horizontal", gap=6, pad=[0, 8, 0, 10], stroke="border", r=7, cross="center")
    text(type_filter, "All types", 13, "Medium", "muted")
    icon(type_filter, "chevron", 14, "muted")
    toggle = box(row, "View", h=32, dir="horizontal", gap=2, pad=2, stroke="border", r=7, cross="center")
    for ic, on in (("cards", not detailed), ("list", detailed)):
        v = box(toggle, f"View/{ic}", w=30, h=26, dir="horizontal", fill="surface-2" if on else "none", r=5, main="center", cross="center")
        icon(v, ic, 15, "text" if on else "muted")
    return row


def footer(root, connected=True, speed="14.2 MB/s", limit="Unlimited", update=False, left="3.7 GB left"):
    foot = box(root, "Status bar", h=36, dir="horizontal", gap=8, pad=[0, 20], fill="surface", cross="center", sw="fill",
               props={"stroke": paint("border"), "strokeWeight": 1, "strokeTopWeight": 1, "strokeRightWeight": 0, "strokeBottomWeight": 0, "strokeLeftWeight": 0})
    box(foot, "Dot", w=8, h=8, fill="success" if connected else "warn", r=4)
    text(foot, "Browser connected" if connected else "Browser not connected", 12, "Medium", "muted")
    if not connected:
        text(foot, "Connect", 12, "Semi Bold", "accent")
    if update:
        text(foot, "·", 12, "Regular", "muted")
        text(foot, "ODM update ready", 12, "Medium", "muted")
        text(foot, "Restart now", 12, "Semi Bold", "accent")
    spacer(foot)
    if left:
        text(foot, left, 12, "Medium", "muted", tnum=True)
        text(foot, "·", 12, "Regular", "muted")
    icon(foot, "download", 14, "muted")
    text(foot, speed, 12, "Semi Bold", "text", tnum=True)
    text(foot, "·", 12, "Regular", "muted")
    text(foot, limit, 12, "Medium", "muted")
    icon(foot, "chevron", 12, "muted")
    return foot


def group(parent, label, count, action=None, w=1140, size=None):
    g = box(parent, f"Group/{label}", w=w, h=26, dir="horizontal", gap=6, pad=[0, 2], cross="center")
    text(g, label, 12, "Semi Bold", "muted")
    text(g, str(count), 12, "Medium", "muted", tnum=True)
    if size:
        text(g, "·", 12, "Regular", "muted")
        text(g, size, 12, "Medium", "muted", tnum=True)
    spacer(g)
    if action:
        text(g, action, 12, "Semi Bold", "muted")
    return g


def row(parent, item, w=1140):
    """One download. Every row carries its own next step, so nothing needs selecting first."""
    state = item.get("state", "active")
    r = box(parent, f"Row/{item['name'][:24]}", w=w, h=68, dir="horizontal", gap=14, pad=[0, 11, 0, 13] if state == "problem" else [0, 12, 0, 14], fill="surface", r=10, cross="center",
            stroke="error" if state == "problem" else None)
    bad = state == "problem"
    if item.get("grip"):
        icon(r, "grip", 18, "muted")
    tile = box(r, "Type", w=40, h=40, dir="horizontal", fill="error-tint" if bad else "surface-2", r=8, main="center", cross="center")
    icon(tile, "alert" if bad else item["icon"], 20, "error" if bad else ("accent" if state == "active" else "text"))
    right = item.get("right", 262)
    midw = w - 14 - 12 - 40 - 14 - 14 - right - (32 if item.get("grip") else 0)
    mid = box(r, "Text", w=midw, dir="vertical", gap=5)
    text(mid, item["name"], 14, "Semi Bold", "text", w=midw, truncate=True, name="Name")
    text(mid, item["sub"], 12, "Regular", "error" if bad else "muted", w=midw, truncate=True, tnum=True, name="Summary")
    if "pct" in item:
        bar(mid, midw, item["pct"], item.get("bar", "accent"))
    side = box(r, "Actions", w=right, dir="horizontal", gap=6, main="max", cross="center")
    if item.get("speed"):
        text(side, item["speed"], 13, "Semi Bold", "text", tnum=True)
        box(side, "Gap", w=6, h=1)
    for action in item.get("actions", []):
        kind = action[0]
        if kind == "icon":
            icon_button(side, action[1], 32, "ghost")
        else:
            button(side, action[1], kind, action[2] if len(action) > 2 else None)
    return r


ROWS = {
    "problem": {"name": "fedora-workstation-41.iso", "icon": "program", "state": "problem", "sub": "The link expired at 38%. Your progress is safe.",
                "actions": [("primary", "Paste new link"), ("icon", "more")]},
    "iso": {"name": "ubuntu-24.04-desktop-amd64.iso", "icon": "program", "sub": "3.1 GB of 5.9 GB · 6 min left", "pct": 52, "speed": "8.4 MB/s",
            "actions": [("icon", "pause"), ("icon", "more")]},
    "video": {"name": "Sintel (2010) 1080p", "icon": "video", "sub": "612 MB of 1.1 GB · 2 min left", "pct": 56, "speed": "5.8 MB/s",
              "actions": [("icon", "pause"), ("icon", "more")]},
    "queued": {"name": "blender-4.5-windows-x64.zip", "icon": "archive", "state": "queued", "sub": "412 MB · Waiting for the queue schedule",
               "actions": [("secondary", "Start now"), ("icon", "more")]},
    "done1": {"name": "annual-report-2026.pdf", "icon": "doc", "state": "done", "sub": "4.2 MB · Finished 12 min ago",
              "actions": [("secondary", "Open"), ("icon", "folder"), ("icon", "more")]},
    "done2": {"name": "Tears of Steel soundtrack.flac", "icon": "music", "state": "done", "sub": "86 MB · Finished today at 09:41",
              "actions": [("secondary", "Open"), ("icon", "folder"), ("icon", "more")]},
}


def screen(name, x, y, W=1180, H=720):
    return box(None, name, w=W, h=H, dir="vertical", fill="bg", x=x, y=y, clip=True, sh="fixed")


def home_list(root, W=1180):
    lst = box(root, "List", dir="vertical", gap=6, pad=[0, 20, 8, 20], sw="fill", sh="fill", clip=True)
    w = W - 40
    group(lst, "Needs you", 1, w=w)
    row(lst, ROWS["problem"], w)
    group(lst, "Downloading", 2, "Pause all", w=w, size="3.3 GB left")
    row(lst, ROWS["iso"], w)
    row(lst, ROWS["video"], w)
    group(lst, "Up next", 1, w=w, size="412 MB")
    row(lst, ROWS["queued"], w)
    group(lst, "Finished", 2, "Clear", w=w)
    row(lst, ROWS["done1"], w)
    row(lst, ROWS["done2"], w)
    return lst


def home(name, x, y, W=1180, H=720):
    root = screen(name, x, y, W, H)
    header(root, W)
    tabs(root)
    home_list(root, W)
    footer(root)
    return root


def scrim(root, W=1180, H=720):
    return box(root, "Scrim", w=W, h=H, fill="#000000B8", x=0, y=0, absolute=True)


def active_paste(root, value):
    paste = box(root, "Paste bar (active)", w=1096, h=44, dir="horizontal", gap=10, pad=[0, 14], fill="surface", stroke="accent", r=10, cross="center", x=64, y=12, absolute=True)
    icon(paste, "download", 18, "accent")
    text(paste, value, 14, "Medium", "text", fill_w=True, truncate=True)
    return paste


def panel(root, name, y=64, x=64, w=1096, gap=14):
    p = box(root, name, w=w, dir="vertical", gap=gap, pad=18, fill="surface", stroke="border", r=12, x=x, y=y, absolute=True)
    cmd("effect.add", {"ids": [ref(p)], "type": "dropShadow"})
    return p


def preview_head(p, ic, title, sub, w=1060, subcolor="muted"):
    h = box(p, "Preview", w=w, dir="horizontal", gap=14, cross="center")
    tile = box(h, "Type", w=48, h=48, dir="horizontal", fill="surface-2", r=9, main="center", cross="center")
    icon(tile, ic, 24, "accent")
    t = box(h, "Text", dir="vertical", gap=3, sw="fill")
    text(t, title, 16, "Semi Bold", "text", fill_w=True, truncate=True, name="Name")
    text(t, sub, 13, "Regular", subcolor, fill_w=True, name="Facts", tnum=True)
    return h


def field(p, label, value, w, chevron=False, action=None, hint=False):
    f = box(p, f"Field/{label}", w=w, dir="vertical", gap=5)
    text(f, label, 12, "Medium", "muted")
    i = box(f, "Input", h=36, dir="horizontal", gap=8, pad=[0, 10], fill="bg", stroke="border", r=7, cross="center", sw="fill")
    text(i, value, 13, "Regular", "muted" if hint else "text", fill_w=True, truncate=True, name="Value")
    if chevron:
        icon(i, "chevron", 14, "muted")
    if action:
        text(i, action, 13, "Semi Bold", "accent")
    return f


def panel_actions(p, primary, w=1060, left="More options", leftic="chevron"):
    a = box(p, "Actions", w=w, dir="horizontal", gap=8, cross="center")
    if left:
        l = box(a, "Disclosure", h=32, dir="horizontal", gap=4, cross="center")
        text(l, left, 13, "Semi Bold", "muted")
        icon(l, leftic, 14, "muted")
    spacer(a)
    button(a, "Cancel", "ghost")
    button(a, primary, "primary", h=36)
    return a


UBUNTU_URL = "https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso"


def paste_file(x, y):
    root = home("04 Paste: file", x, y)
    scrim(root)
    active_paste(root, UBUNTU_URL)
    p = panel(root, "Add panel")
    preview_head(p, "program", "ubuntu-24.04-desktop-amd64.iso", "5.9 GB · Can pause and resume")
    place = box(p, "Destination", w=1060, h=40, dir="horizontal", gap=8, pad=[0, 12], fill="bg", r=8, cross="center")
    icon(place, "folder", 16, "muted")
    text(place, "Downloads\\Programs", 13, "Medium", "text")
    spacer(place)
    text(place, "Change", 13, "Semi Bold", "accent")
    panel_actions(p, "Download")
    return root


def paste_options(x, y):
    root = home("05 Paste: more options", x, y)
    scrim(root)
    active_paste(root, UBUNTU_URL)
    p = panel(root, "Add panel")
    preview_head(p, "program", "ubuntu-24.04-desktop-amd64.iso", "5.9 GB · Can pause and resume")
    r1 = box(p, "Row", w=1060, dir="horizontal", gap=12)
    field(r1, "File name", "ubuntu-24.04-desktop-amd64.iso", 524)
    field(r1, "Save to", "Downloads\\Programs", 524, action="Browse")
    r2 = box(p, "Row", w=1060, dir="horizontal", gap=12)
    field(r2, "Type", "Automatic (program)", 345, chevron=True)
    field(r2, "Connections", "8", 345)
    field(r2, "Speed limit", "Unlimited", 345, hint=True)
    a = box(p, "Timing", w=1060, dir="horizontal", gap=8, cross="center")
    check = box(a, "Checkbox", w=18, h=18, stroke="border", r=4, fill="bg")
    text(a, "Add to the queue instead of starting now", 13, "Regular", "text")
    panel_actions(p, "Download", left="Fewer options", leftic="chevron-right")
    return root


def paste_video(x, y):
    root = home("06 Paste: video page", x, y)
    scrim(root)
    active_paste(root, "https://www.youtube.com/watch?v=eRsGyueVLvQ")
    p = panel(root, "Add panel")
    preview_head(p, "video", "Sintel", "Video from a web page · Saved as MP4")
    text(p, "Quality", 12, "Medium", "muted")
    chips = box(p, "Quality", w=1060, dir="horizontal", gap=8)
    for label, on in (("Best", True), ("1080p", False), ("720p", False), ("480p", False), ("Audio only", False)):
        c = box(chips, f"Quality/{label}", h=44, dir="horizontal", pad=[0, 12], fill="selected" if on else "bg", stroke="accent" if on else "border",
                r=8, main="center", cross="center", sw="fill")
        text(c, label, 14, "Semi Bold", "text")
    subs = box(p, "Subtitles", w=1060, dir="horizontal", gap=8, cross="center")
    check = box(subs, "Checkbox", w=18, h=18, dir="horizontal", fill="accent", r=4, main="center", cross="center")
    icon(check, "check", 13, "on-accent", 2.5)
    text(subs, "Include subtitles when available", 13, "Regular", "text")
    panel_actions(p, "Download video")
    return root


def paste_torrent(x, y):
    root = home("07 Paste: torrent", x, y)
    scrim(root)
    active_paste(root, "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c")
    p = panel(root, "Add panel")
    preview_head(p, "magnet", "Big Buck Bunny (2008)", "Torrent · 4 of 5 files · 1.3 GB of 2.1 GB")
    tree = box(p, "Files", w=1060, dir="vertical", fill="bg", r=8, clip=True)
    files = [("Big Buck Bunny 1080p.mkv", "1.2 GB", True, "video"), ("Big Buck Bunny 2160p.mkv", "820 MB", False, "video"),
             ("Soundtrack.flac", "64 MB", True, "music"), ("Poster.jpg", "3.2 MB", True, "image"), ("Credits.txt", "4 KB", True, "doc")]
    for name, size, on, ic in files:
        f = box(tree, f"File/{name}", h=38, dir="horizontal", gap=10, pad=[0, 12], cross="center", sw="fill")
        check = box(f, "Checkbox", w=18, h=18, dir="horizontal", fill="accent" if on else "none", stroke=None if on else "border", r=4, main="center", cross="center")
        if on:
            icon(check, "check", 13, "on-accent", 2.5)
        icon(f, ic, 16, "muted")
        text(f, name, 13, "Regular", "text" if on else "muted", fill_w=True, truncate=True)
        text(f, size, 13, "Regular", "muted", tnum=True)
    text(p, "ODM stops sharing once your files are finished.", 12, "Regular", "muted")
    panel_actions(p, "Download 4 files", left="Select all", leftic="check")
    return root


def details(x, y):
    root = home("08 Download details", x, y)
    box(root, "Dim", w=1180, h=720, fill="#00000080", x=0, y=0, absolute=True)
    d = box(root, "Details drawer", w=400, h=720, dir="vertical", gap=16, pad=20, fill="surface", x=780, y=0, absolute=True, sh="fixed",
            props={"stroke": paint("border"), "strokeWeight": 1, "strokeLeftWeight": 1, "strokeTopWeight": 0, "strokeRightWeight": 0, "strokeBottomWeight": 0})
    cmd("effect.add", {"ids": [ref(d)], "type": "dropShadow"})
    head = box(d, "Title", w=360, dir="horizontal", gap=8, cross="center")
    text(head, "ubuntu-24.04-desktop-amd64.iso", 15, "Semi Bold", "text", fill_w=True, truncate=True)
    icon_button(head, "x", 32, "ghost")
    hero = box(d, "Progress", w=360, dir="vertical", gap=8)
    pct = box(hero, "Percent", w=360, dir="horizontal", cross="baseline", gap=8)
    text(pct, "52%", 34, "Bold", "text", tnum=True)
    text(pct, "3.1 GB of 5.9 GB", 13, "Regular", "muted", tnum=True)
    bar(hero, 360, 52, h=6)
    stats = box(d, "Stats", w=360, dir="horizontal", gap=8)
    for label, value in (("Speed", "8.4 MB/s"), ("Time left", "6 min")):
        s = box(stats, f"Stat/{label}", dir="vertical", gap=2, pad=[10, 12], fill="bg", r=8, sw="fill")
        text(s, label, 12, "Medium", "muted")
        text(s, value, 15, "Semi Bold", "text", tnum=True)
    act = box(d, "Actions", w=360, dir="horizontal", gap=8)
    button(act, "Pause", "primary", "pause", h=36, full=True)
    button(act, "Open folder", "secondary", "folder", h=36, full=True)
    icon_button(act, "copy", 36)
    icon_button(act, "trash", 36)
    info = box(d, "Info", w=360, dir="vertical", gap=10)
    for label, value in (("Saved to", "Downloads\\Programs"), ("Speed limit", "Unlimited")):
        l = box(info, f"Info/{label}", w=360, dir="horizontal", cross="center")
        text(l, label, 13, "Regular", "muted")
        spacer(l)
        text(l, value, 13, "Medium", "text")
    box(d, "Divider", w=360, h=1, fill="border")
    tech = box(d, "Technical", w=360, dir="vertical", gap=10)
    th = box(tech, "Disclosure", w=360, dir="horizontal", cross="center")
    text(th, "Technical details", 13, "Semi Bold", "muted")
    spacer(th)
    icon(th, "chevron", 14, "muted")
    chunks = box(tech, "Chunk map", w=360, h=14, dir="horizontal", gap=2)
    for done in (1, 1, 1, 1, 1, 1, 0.5, 1, 1, 0.5, 1, 1, 0, 0, 0.5, 0, 0, 0, 0, 0):
        box(chunks, "Chunk", h=14, fill="accent" if done == 1 else ("accent-light" if done else "surface-2"), r=2, sw="fill")
    for label, value in (("Connections", "Up to 8"), ("Address", "releases.ubuntu.com/24.04/…")):
        l = box(tech, f"Info/{label}", w=360, dir="horizontal", cross="center")
        text(l, label, 13, "Regular", "muted")
        spacer(l)
        text(l, value, 13, "Regular", "muted")
    return root


def problem_card(parent, ic, name, title, body, actions, w=1140):
    c = box(parent, f"Problem/{name}", w=w, dir="vertical", gap=12, pad=16, fill="surface", r=10)
    top = box(c, "Top", w=w - 32, dir="horizontal", gap=14)
    tile = box(top, "Type", w=40, h=40, dir="horizontal", fill="error-tint", r=8, main="center", cross="center")
    icon(tile, ic, 20, "error")
    t = box(top, "Text", dir="vertical", gap=3, sw="fill")
    text(t, title, 15, "Semi Bold", "text", name="Title")
    text(t, name, 12, "Regular", "muted", name="File", tnum=True)
    text(t, body, 13, "Regular", "muted", fill_w=True, lh="150%", name="What to do")
    side = box(top, "Actions", dir="horizontal", gap=6, cross="center")
    for a in actions:
        button(side, a[1], a[0], h=34)
    return c


def problems(x, y):
    root = screen("09 Problems", x, y)
    header(root, 1180)
    tabs(root, "Problems", counts=[("All", 9), ("Downloading", 2), ("Up next", 1), ("Finished", 3), ("Problems", 3)])
    lst = box(root, "List", dir="vertical", gap=8, pad=[4, 20, 8, 20], sw="fill", sh="fill")
    problem_card(lst, "refresh", "fedora-workstation-41.iso · stopped at 38% of 2.3 GB", "The download link expired",
                 "Sites often hand out links that only last a while. Start the download again in your browser and paste the new link here. ODM picks up where it stopped.",
                 [("primary", "Paste new link"), ("ghost", "Remove")])
    problem_card(lst, "video", "Tears of Steel (2012)", "This video needs the video tools",
                 "FFmpeg and yt-dlp save videos from sites. They are free and only need installing once.",
                 [("primary", "Install (150 MB)"), ("ghost", "Remove")])
    problem_card(lst, "globe", "debian-13-amd64-DVD-1.iso · stopped at 71% of 3.8 GB", "The server stopped responding",
                 "ODM tried several times, then stopped so it would not waste your connection. It usually works again in a few minutes.",
                 [("primary", "Try again"), ("ghost", "Remove")])
    footer(root)
    return root


COLS = [("Name", 410, "left"), ("Size", 90, "right"), ("Progress", 200, "left"), ("Speed", 96, "right"), ("Time left", 84, "right"), ("Status", 120, "left"), ("Added", 100, "right")]


def table_row(parent, cells, pct=None, selected=False, header_row=False, color="text"):
    r = box(parent, "Table row", w=1140, h=36, dir="horizontal", pad=[0, 12, 0, 12], cross="center", fill="selected" if selected else "none",
            props={"stroke": paint("border"), "strokeWeight": 1, "strokeBottomWeight": 1, "strokeTopWeight": 0, "strokeLeftWeight": 0, "strokeRightWeight": 0})
    for i, ((label, w, align), value) in enumerate(zip(COLS, cells)):
        cell = box(r, f"Cell/{label}", w=w, h=36, dir="horizontal", gap=8, pad=[0, 8], cross="center", main="max" if align == "right" else None)
        if label == "Progress" and pct is not None:
            bar(cell, 132, pct, "error" if cells[5] == "Problem" else "accent")
            text(cell, f"{pct}%", 12, "Regular", "muted", tnum=True)
        else:
            c = "muted" if header_row or i > 0 else color
            if label == "Status" and value == "Problem":
                c = "error"
            text(cell, value, 12 if header_row else 13, "Semi Bold" if header_row else ("Medium" if i == 0 else "Regular"), c, tnum=True,
                 w=w - 16 if i == 0 else None, truncate=i == 0)
    return r


def detailed(x, y):
    root = screen("10 Detailed view", x, y)
    header(root, 1180)
    tabs(root, detailed=True)
    lst = box(root, "Table", dir="vertical", pad=[0, 20], sw="fill", sh="fill")
    table_row(lst, [c[0] for c in COLS], header_row=True)
    table_row(lst, ["fedora-workstation-41.iso", "2.3 GB", "", "—", "—", "Problem", "Today 08:12"], 38)
    table_row(lst, ["ubuntu-24.04-desktop-amd64.iso", "5.9 GB", "", "8.4 MB/s", "6 min", "Downloading", "Today 10:31"], 52, selected=True)
    table_row(lst, ["Sintel (2010) 1080p", "1.1 GB", "", "5.8 MB/s", "2 min", "Downloading", "Today 10:33"], 56, selected=True)
    table_row(lst, ["blender-4.5-windows-x64.zip", "412 MB", "", "—", "—", "Up next", "Today 10:34"], 0)
    table_row(lst, ["annual-report-2026.pdf", "4.2 MB", "", "—", "—", "Finished", "Today 10:02"], 100)
    table_row(lst, ["Tears of Steel soundtrack.flac", "86 MB", "", "—", "—", "Finished", "Today 09:41"], 100)
    bar_ = box(root, "Selection bar", h=48, dir="horizontal", gap=6, pad=[0, 8, 0, 16], fill="surface-2", stroke="border", r=10, cross="center", x=320, y=616, absolute=True, w=540)
    cmd("effect.add", {"ids": [ref(bar_)], "type": "dropShadow"})
    text(bar_, "2 selected", 13, "Semi Bold", "text")
    spacer(bar_)
    button(bar_, "Pause", "secondary", "pause")
    button(bar_, "Open folder", "secondary", "folder")
    button(bar_, "Remove", "secondary", "trash")
    icon_button(bar_, "x", 32, "ghost")
    footer(root, update=True)
    return root


def setting(parent, title, desc, control, w=720):
    r = box(parent, f"Setting/{title}", w=w, h=62, dir="horizontal", gap=16, pad=[0, 16], cross="center")
    t = box(r, "Text", dir="vertical", gap=2, sw="fill")
    text(t, title, 14, "Medium", "text")
    text(t, desc, 12, "Regular", "muted", fill_w=True)
    kind = control[0]
    if kind == "switch":
        on = control[1]
        s = box(r, "Switch", w=38, h=22, dir="horizontal", pad=3, fill="accent" if on else "surface-2", r=11, cross="center", main="max" if on else "min")
        box(s, "Knob", w=16, h=16, fill="on-accent" if on else "muted", r=8)
    elif kind == "select":
        s = box(r, "Select", h=34, dir="horizontal", gap=8, pad=[0, 10], fill="bg", stroke="border", r=7, cross="center")
        text(s, control[1], 13, "Medium", "text")
        icon(s, "chevron", 14, "muted")
    else:
        button(r, control[1], "secondary", h=34)
    return r


def settings(x, y):
    root = box(None, "11 Settings", w=1180, h=720, dir="horizontal", fill="bg", x=x, y=y, clip=True, sh="fixed")
    nav = box(root, "Sections", w=236, dir="vertical", gap=2, pad=[16, 12], fill="surface", sh="fill")
    back = box(nav, "Back", h=36, dir="horizontal", gap=8, pad=[0, 8], cross="center", sw="fill")
    icon(back, "back", 16, "muted")
    text(back, "Settings", 15, "Bold", "text")
    search = box(nav, "Search", h=36, dir="horizontal", gap=8, pad=[0, 10], fill="bg", stroke="border", r=7, cross="center", sw="fill")
    icon(search, "search", 15, "muted")
    text(search, "Find a setting", 13, "Regular", "muted")
    box(nav, "Gap", h=8, w=10)
    for label, ic, on in (("Essentials", "cards", True), ("Downloads", "download", False), ("Browser", "globe", False), ("Video", "video", False),
                          ("Torrents", "magnet", False), ("Advanced", "sliders", False)):
        n = box(nav, f"Nav/{label}", h=36, dir="horizontal", gap=10, pad=[0, 10], fill="surface-2" if on else "none", r=7, cross="center", sw="fill")
        icon(n, ic, 16, "text" if on else "muted")
        text(n, label, 13, "Semi Bold", "text" if on else "muted")
    main = box(root, "Content", dir="vertical", gap=16, pad=[28, 40], sw="fill", sh="fill")
    text(main, "Essentials", 22, "Bold", "text")
    text(main, "The few things most people change. Everything else is one section away.", 13, "Regular", "muted")
    card = box(main, "Card", w=720, dir="vertical", fill="surface", r=10)
    setting(card, "Save downloads to", "C:\\Users\\You\\Downloads", ("button", "Change"))
    setting(card, "Sort into folders by type", "Videos, Programs, Documents and so on", ("switch", True))
    setting(card, "Downloads at once", "The rest wait their turn", ("select", "3"))
    setting(card, "Speed limit", "Leaves room for everything else on your connection", ("select", "Unlimited"))
    setting(card, "Tell me when a download finishes", "A small notification, even from the tray", ("switch", True))
    setting(card, "Keep running in the tray when closed", "Downloads carry on after you close the window", ("switch", True))
    setting(card, "Keep ODM up to date", "Checks for new versions in the background", ("switch", True))
    return root


def welcome_shell(name, x, y, step, title, body):
    root = screen(name, x, y)
    top = box(root, "Top", h=68, dir="horizontal", gap=10, pad=[0, 24], cross="center", sw="fill")
    logo(top, 28)
    text(top, "ODM", 15, "Bold", "text")
    spacer(top)
    text(top, "Skip setup", 13, "Semi Bold", "muted")
    mid = box(root, "Step", dir="vertical", gap=24, pad=[0, 0, 68, 0], sw="fill", sh="fill", cross="center", main="center")
    dots = box(mid, "Steps", dir="horizontal", gap=6)
    for i in range(3):
        box(dots, "Step dot", w=40, h=4, fill="accent" if i <= step else "surface-2", r=2)
    t = box(mid, "Title", w=620, dir="vertical", gap=8, cross="center")
    text(t, title, 28, "Bold", "text", align="center")
    text(t, body, 15, "Regular", "muted", w=620, align="center", lh="150%")
    return root, mid


def welcome_one(x, y):
    root, mid = welcome_shell("01 Welcome: browser", x, y, 0, "Which browser do you use?", "ODM connects to it, so downloads and videos open here on their own.")
    grid = box(mid, "Browsers", w=620, dir="horizontal", gap=10, props={"wrap": True, "crossGap": 10}, main="center")
    for b in ("Chrome", "Edge", "Brave", "Vivaldi", "Opera", "Firefox", "Floorp"):
        on = b == "Chrome"
        c = box(grid, f"Browser/{b}", w=146, h=60, dir="horizontal", fill="selected" if on else "surface", stroke="accent" if on else "border", r=10, main="center", cross="center")
        text(c, b, 14, "Semi Bold", "text")
    button(mid, "Continue", "primary", h=44, size=15)
    return root


def welcome_two(x, y):
    root, mid = welcome_shell("02 Welcome: connect", x, y, 1, "Add ODM to Chrome", "Three quick steps. The folder is already open, and this page moves on by itself once Chrome connects.")
    card = box(mid, "Instructions", w=620, dir="vertical", fill="surface", r=10)
    for i, (head, body) in enumerate((("Open chrome://extensions", "Then turn on Developer mode, top right"), ("Choose Load unpacked", "It is the first button that appears"), ("Pick the folder that just opened", "ODM has it ready"))):
        r = box(card, f"Instruction/{i + 1}", h=64, dir="horizontal", gap=14, pad=[0, 16], cross="center", sw="fill")
        n = box(r, "Number", w=28, h=28, dir="horizontal", fill="surface-2", r=14, main="center", cross="center")
        text(n, str(i + 1), 13, "Bold", "text")
        t = box(r, "Text", dir="vertical", gap=1, sw="fill")
        text(t, head, 14, "Semi Bold", "text")
        text(t, body, 12, "Regular", "muted")
        if i == 0:
            button(r, "Open page", "secondary")
    live = box(mid, "Live status", h=44, dir="horizontal", gap=10, pad=[0, 16], fill="bg", stroke="border", r=22, cross="center")
    box(live, "Dot", w=8, h=8, fill="accent", r=4)
    text(live, "Waiting for Chrome…", 13, "Medium", "muted")
    foot = box(mid, "Footer", dir="horizontal", gap=8, cross="center")
    button(foot, "Back", "ghost", h=40)
    button(foot, "Do this later", "ghost", h=40)
    return root


def welcome_three(x, y):
    root, mid = welcome_shell("02b Welcome: video tools", x, y, 2, "Want to save videos from sites?", "FFmpeg and yt-dlp are free tools that let ODM save YouTube and other site videos. You only install them once.")
    ok = box(mid, "Connected", h=44, dir="horizontal", gap=10, pad=[0, 16], fill="success-tint", r=22, cross="center")
    icon(ok, "check", 16, "success", 2.5)
    text(ok, "Chrome connected", 13, "Semi Bold", "success")
    foot = box(mid, "Footer", dir="horizontal", gap=8, cross="center")
    button(foot, "Not now", "ghost", h=44, size=15)
    button(foot, "Install (about 150 MB)", "primary", h=44, size=15)
    return root


def empty_home(x, y):
    root = screen("03 Home: first launch", x, y)
    header(root, 1180, focused=True)
    mid = box(root, "Empty", dir="vertical", gap=24, pad=[64, 0, 0, 0], sw="fill", sh="fill", cross="center")
    t = box(mid, "Title", w=620, dir="vertical", gap=8, cross="center")
    text(t, "Paste a link to start", 28, "Bold", "text", align="center")
    text(t, "ODM reads the link, shows you what it found and saves it to your Downloads folder.", 15, "Regular", "muted", w=620, align="center", lh="150%")
    cards = box(mid, "Ways in", dir="horizontal", gap=12)
    for ic, head, body, cta in (("download", "Any file", "Paste a link above, or press Ctrl V anywhere in ODM.", None),
                                ("globe", "From your browser", "Downloads and videos come here on their own.", "Connect browser"),
                                ("magnet", "Torrents", "Paste a magnet link, or drop a .torrent file on this window.", None)):
        c = box(cards, f"Way/{head}", w=300, h=168, dir="vertical", gap=8, pad=18, fill="surface", r=12, sh="fixed")
        tile = box(c, "Icon", w=36, h=36, dir="horizontal", fill="surface-2", r=8, main="center", cross="center")
        icon(tile, ic, 18, "accent")
        text(c, head, 15, "Semi Bold", "text")
        text(c, body, 13, "Regular", "muted", w=264, lh="145%")
        if cta:
            text(c, cta, 13, "Semi Bold", "accent")
    footer(root, connected=False, speed="0 B/s")
    return root


SPEED_LABELS = (("Unlimited", True), ("512 KB/s", False), ("1 MB/s", False), ("2 MB/s", False), ("5 MB/s", False), ("Custom…", False))


def speed_menu(x, y):
    root = home("12 Speed menu", x, y)
    m = box(root, "Speed menu", w=280, dir="vertical", gap=2, pad=6, fill="surface", stroke="border", r=10, x=880, y=330, absolute=True)
    cmd("effect.add", {"ids": [ref(m)], "type": "dropShadow"})
    for label, on in SPEED_LABELS:
        r = box(m, f"Speed/{label}", h=34, dir="horizontal", gap=8, pad=[0, 10], fill="surface-2" if on else "none", r=6, cross="center", sw="fill")
        if on:
            icon(r, "check", 16, "accent", 2.25)
        else:
            box(r, "Mark", w=16, h=16)
        text(r, label, 13, "Medium", "text")
    box(m, "Divider", h=1, fill="border", sw="fill")
    for ic, label in (("clock", "Limit only at certain times"), ("pause", "Pause all")):
        r = box(m, label, h=34, dir="horizontal", gap=8, pad=[0, 10], r=6, cross="center", sw="fill")
        icon(r, ic, 16, "muted")
        text(r, label, 13, "Medium", "text")
    return root


def row_menu(x, y):
    root = home("12b Row menu", x, y)
    m = box(root, "Row menu", w=220, dir="vertical", gap=2, pad=6, fill="surface", stroke="border", r=10, x=920, y=290, absolute=True)
    cmd("effect.add", {"ids": [ref(m)], "type": "dropShadow"})
    for ic, label, color in (("cards", "Details", "text"), ("folder", "Open folder", "text"), ("copy", "Copy link", "text"), ("refresh", "Paste new link", "text"),
                             ("x", "Cancel download", "text"), ("trash", "Remove from list", "error")):
        r = box(m, label, h=34, dir="horizontal", gap=8, pad=[0, 10], r=6, cross="center", sw="fill")
        icon(r, ic, 16, "muted" if color == "text" else color)
        text(r, label, 13, "Medium", color)
    return root


def queue_order(x, y):
    root = screen("12c Up next: choose the order", x, y)
    header(root, 1180)
    tabs(root, "Up next", counts=[("All", 8), ("Downloading", 2), ("Up next", 3), ("Finished", 2), ("Problems", 1)])
    lst = box(root, "List", dir="vertical", gap=6, pad=[0, 20, 8, 20], sw="fill", sh="fill", clip=True)
    group(lst, "Up next", 3, "Sort by size", size="1.9 GB")
    items = [
        {"name": "blender-4.5-windows-x64.zip", "icon": "archive", "sub": "412 MB · 1st in line"},
        {"name": "node-v24-x64.msi", "icon": "program", "sub": "31 MB · 2nd in line"},
        {"name": "debian-13-amd64-netinst.iso", "icon": "program", "sub": "1.4 GB · 3rd in line"},
    ]
    for item in items:
        row(lst, {**item, "state": "queued", "grip": True, "actions": [("icon", "pause"), ("icon", "more")]})
    m = box(root, "Row menu", w=220, dir="vertical", gap=2, pad=6, fill="surface", stroke="border", r=10, x=920, y=216, absolute=True)
    cmd("effect.add", {"ids": [ref(m)], "type": "dropShadow"})
    for ic, label in (("top", "Move to top"), ("up", "Move up"), ("down", "Move down")):
        r = box(m, label, h=34, dir="horizontal", gap=8, pad=[0, 10], r=6, cross="center", sw="fill")
        icon(r, ic, 16, "muted")
        text(r, label, 13, "Medium", "text")
    box(m, "Divider", h=1, fill="border", sw="fill")
    for ic, label, color in (("cards", "Details", "text"), ("trash", "Remove from list", "error")):
        r = box(m, label, h=34, dir="horizontal", gap=8, pad=[0, 10], r=6, cross="center", sw="fill")
        icon(r, ic, 16, "muted" if color == "text" else color)
        text(r, label, 13, "Medium", color)
    footer(root, left="5.2 GB left")
    return root


def extension():
    page = box(None, "Extension: on-page controls", w=1180, h=520, dir="horizontal", gap=40, pad=40, fill="bg", x=0, y=0)
    video = box(page, "Video hover", w=520, h=300, fill="surface", r=10)
    text(video, "A video on any page", 13, "Regular", "muted", x=16, y=264)
    hover = box(video, "Hover button", dir="horizontal", gap=0, fill="surface", stroke="accent", r=8, x=258, y=12, clip=True)
    for label, name in (("MP4", "Video"), ("MP3", "Audio")):
        b = box(hover, f"Format/{name}", h=34, dir="horizontal", gap=6, pad=[0, 12], cross="center")
        if name == "Video":
            icon(b, "download", 14, "accent")
        text(b, label, 13, "Bold", "text")
        box(hover, "Divider", w=1, h=34, fill="border")
    close = box(hover, "Hide", w=34, h=34, dir="horizontal", main="center", cross="center")
    icon(close, "x", 14, "muted")
    side = box(page, "Page bar", w=360, dir="vertical", gap=8, cross="max")
    lst = box(side, "List", w=340, dir="vertical", gap=10, pad=10, fill="bg", stroke="border", r=8)
    for name in ("trailer-1080p.mp4", "stream (HLS)"):
        r = box(lst, name, w=318, h=32, dir="horizontal", gap=6, cross="center")
        text(r, name, 13, "Regular", "text", fill_w=True, truncate=True)
        button(r, "MP4", "primary", h=28, size=12)
        button(r, "MP3", "secondary", h=28, size=12)
    bar = box(side, "Bar", dir="horizontal", fill="surface", stroke="accent", r=8, cross="center", clip=True)
    grip = box(bar, "Drag", w=28, h=34, dir="horizontal", main="center", cross="center")
    icon(grip, "grip", 16, "muted")
    b = box(bar, "Main", h=34, dir="horizontal", pad=[0, 10], cross="center")
    text(b, "ODM · 2 videos", 13, "Bold", "text")
    box(bar, "Divider", w=1, h=34, fill="border")
    c = box(bar, "Hide", w=34, h=34, dir="horizontal", main="center", cross="center")
    icon(c, "x", 14, "muted")
    return page


def compact(x, y):
    W, H = 900, 560
    root = screen("13 Smallest window (900 × 560)", x, y, W, H)
    header(root, W)
    tabs(root)
    lst = box(root, "List", dir="vertical", gap=6, pad=[0, 20, 8, 20], sw="fill", sh="fill", clip=True)
    w = W - 40
    group(lst, "Needs you", 1, w=w)
    row(lst, {**ROWS["problem"], "right": 170}, w)
    group(lst, "Downloading", 2, "Pause all", w=w)
    row(lst, {**ROWS["iso"], "right": 170}, w)
    row(lst, {**ROWS["video"], "right": 170}, w)
    footer(root)
    return root


def foundations():
    root = box(None, "Foundations", w=1180, dir="vertical", gap=28, pad=40, fill="bg", x=0, y=0)
    text(root, "ODM suite foundations", 28, "Bold", "text")
    text(root, "Colours keep their app.css names. Orange means the next step, so most screens show it once.", 15, "Regular", "muted")
    swatches = box(root, "Colours", w=1100, dir="horizontal", gap=10, props={"wrap": True, "crossGap": 14})
    for key, value in TOKENS.items():
        s = box(swatches, f"Colour/{key}", w=100, dir="vertical", gap=4)
        box(s, "Swatch", w=100, h=52, fill=key, stroke="border", r=8)
        text(s, key, 12, "Medium", "text")
        text(s, value, 11, "Regular", "muted")
    scale = box(root, "Type", w=1100, dir="vertical", gap=8)
    for size, weight, sample in ((28, "Bold", "Screen title 28"), (22, "Bold", "Section title 22"), (15, "Semi Bold", "Card title 15"), (14, "Semi Bold", "Row name 14"),
                                 (13, "Regular", "Body and controls 13"), (12, "Regular", "Summary and metadata 12")):
        text(scale, f"Inter {weight}: {sample}", size, weight, "text")
    made = []
    comps = box(root, "Components", w=1100, dir="horizontal", gap=12, cross="center", props={"wrap": True, "crossGap": 12})
    for args in (("Download", "primary", "download", 36), ("Open", "secondary", None, 32), ("Clear", "ghost", None, 32), ("Remove", "danger", None, 32)):
        made.append(button(comps, args[0], args[1], args[2], args[3]))
    made.append(icon_button(comps, "pause", 32, "ghost"))
    made.append(pill(comps, "2 selected"))
    for key in ("problem", "iso", "queued", "done1"):
        made.append(row(root, ROWS[key], 1100))
    for m in made:
        cmd("component.create", {"ids": [ref(m)]})


def main():
    cmd("variable.createCollection", {"name": "ODM"}, "tokens")
    for key, value in TOKENS.items():
        cmd("variable.create", {"collection": ref("tokens"), "name": key, "type": "color", "value": value})
    cmd("page.rename", {"name": "Screens"})
    cmd("page.setBackground", {"color": "#111111"})

    cols = [0, 1300, 2600]
    rows = [0, 840, 1680, 2520, 3360, 4200]
    welcome_one(cols[0], rows[0])
    welcome_two(cols[1], rows[0])
    welcome_three(cols[2], rows[0])
    paste_file(cols[0], rows[1])
    paste_options(cols[1], rows[1])
    paste_video(cols[2], rows[1])
    paste_torrent(cols[0], rows[2])
    home("00 Home", cols[1], rows[2])
    details(cols[2], rows[2])
    problems(cols[0], rows[3])
    detailed(cols[1], rows[3])
    settings(cols[2], rows[3])
    speed_menu(cols[0], rows[4])
    row_menu(cols[1], rows[4])
    empty_home(cols[2], rows[4])
    compact(cols[0], rows[5])
    queue_order(cols[1], rows[5])

    cmd("page.add", {"name": "Foundations"}, "foundations")
    cmd("page.select", {"id": ref("foundations")})
    cmd("page.setBackground", {"color": "#111111"})
    foundations()
    cmd("page.add", {"name": "Extension"}, "extension_page")
    cmd("page.select", {"id": ref("extension_page")})
    cmd("page.setBackground", {"color": "#111111"})
    extension()

    script = HERE / "odm-suite.json"
    script.write_text(json.dumps(steps), encoding="utf-8")
    out = HERE / "odm-suite.figcraft"
    run = subprocess.run([CLI, "run", str(script), "--out", str(out)], capture_output=True, text=True, encoding="utf-8")
    lines = run.stdout.strip().splitlines()
    if run.returncode != 0:
        print("\n".join(lines[-3:])[-1500:])
        print(run.stderr[-1500:])
        failed = [json.loads(l).get("step") for l in lines if '"ok":false' in l]
        if failed and failed[0] is not None:
            print(json.dumps(steps[failed[0]])[:900])
        sys.exit(1)
    png = HERE / "png"
    png.mkdir(exist_ok=True)
    for f in png.glob("*.png"):
        f.unlink()
    subprocess.run([CLI, "export", str(out), "-o", str(png), "--page", "Screens"], check=True)
    subprocess.run([CLI, "export", str(out), "-o", str(png), "--page", "Foundations"], check=True)
    subprocess.run([CLI, "export", str(out), "-o", str(png), "--page", "Extension"], check=True)
    subprocess.run([CLI, "render", str(out), "-o", str(HERE / "overview.png"), "--page", "Screens", "--scale", "0.5"], check=True)
    print(len(steps), "steps;", len(list(png.glob("*.png"))), "frames exported")


main()
