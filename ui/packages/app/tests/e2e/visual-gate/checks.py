"""Checks consume recorded values; missing evidence stays NOT RUN."""
from collections import defaultdict
from PIL import Image, ImageChops, ImageFilter
from state_checks import check_state
from loader_checks import check_recording
import hashlib
import re

VIEWS = {"conversation": "conversation", "activity": "activity", "wake-up": "scheduled", "error": "failure",
         "new-fleet": "empty", "models": "models", "team": "team", "security": "security", "usage": "usage",
         "secrets": "secrets", "plugins": "plugins", "connected-apps": "connections"}
DIALOGS = {"create-model-new-key": "models-dialog-[data-action=create-model]", "create-model-saved-key": "create-model-saved-key",
           "invite-member": "team-dialog-[data-team-action=invite]", "create-api-key": "security-dialog-[data-action=create-key]",
           "revoke-api-key": "security-dialog-[data-revoke-key]", "sign-out-session": "security-dialog-[data-revoke-session]",
           "sign-out-other-sessions": "security-dialog-[data-action=revoke-other-sessions]", "add-secret": "secrets-dialog-[data-action=add-secret]",
           "plugin-details": "plugins-dialog-[data-plugin]", "connected-app-details": "connections-dialog-[data-action=connection-details]"}


def near(actual, expected):
    return abs(actual - expected) <= 1


def reference_for(review, capture):
    name = capture["name"].removesuffix("-bottom").removesuffix("-top")
    view = DIALOGS.get(name, VIEWS.get(name))
    return next((row for row in review.reference["results"] if row["view"] == view
                 and row["width"] == capture["width"] and row["theme"] == capture["theme"]), None)


def check_capture(review, capture):
    name, g = capture["captureFilename"], capture["geometry"]
    revision, digest = review.measured["revision"], review.measured["sourceTreeSha256"]
    review.record("A4", name, [revision, digest], [capture["revision"], capture["sourceTreeSha256"]],
                  capture["revision"] == revision and capture["sourceTreeSha256"] == digest, "source fingerprint")
    review.record("A7", name, False, g["overflow"], not g["overflow"], "viewport scroll width")
    review.record("C1", name, "loaded Manrope weights 400/500/600/650", capture.get("fontsLoaded"), capture.get("fontsLoaded") is True, "font load check")
    reference = reference_for(review, capture)
    for dialog in g["dialogs"]:
        check_dialog(review, capture, dialog, reference)
    for eyebrow in g.get("audit", {}).get("eyebrows", []):
        value = eyebrow.get("text") or ""
        if value and re.search(r"[A-Za-z]", value):
            review.record("C12", name, "sentence case", value, value != value.upper(), "owner C12")
    review.record("E10", name, [], g.get("audit", {}).get("redundantButtonTitles", []),
                  not g.get("audit", {}).get("redundantButtonTitles", []), "owner E10")
    if reference:
        check_reference(review, capture, reference)
    check_controls(review, capture)
    check_fleet(review, capture, reference)
    check_state(review, capture, reference)


def check_dialog(review, capture, dialog, reference):
    name, rect = capture["captureFilename"], dialog["rect"]
    title = (dialog.get("title") or {}).get("text", "")
    wide = title == "Create model"
    expected_width = min(capture["width"] - 32, 620 if wide else 540)
    review.record("F1", name, expected_width, rect["width"], near(rect["width"], expected_width), "reference dialog width")
    padding = "20px" if wide and capture["width"] == 390 else "24px"
    review.record("F2", name, padding, rect["paddingLeft"], rect["paddingLeft"] == padding, "proposal; accepted D4")
    close, heading = dialog.get("close"), dialog.get("titleRow")
    actual = None if not close or not heading else [close["width"], close["height"], rect["right"] - close["right"], heading["height"]]
    review.record("F3", name, [40, 40, 17, 40], actual,
                  actual is not None and all(near(a, b) for a, b in zip(actual, [40, 40, 17, 40])), "reference title/close")
    buttons = dialog["footerButtons"]
    if buttons:
        tops = [button["y"] for button in buttons]
        gaps = [right["x"] - left["right"] for left, right in zip(buttons, buttons[1:])]
        review.record("F5", name, "one row; 8px gaps", {"tops": tops, "gaps": gaps},
                      max(tops) - min(tops) <= 1 and all(near(gap, 8) for gap in gaps), "reference modal actions")
        height = 44 if wide or capture["width"] == 390 else 40
        for button in buttons:
            review.record("E2" if capture["width"] == 390 else "E1", name, height, {"button": button["text"], "height": button["height"]},
                          button["height"] == height, "proposal; accepted D3")
    active = dialog.get("activeElement")
    if rect.get("role") == "alertdialog":
        review.record("F9", name, "Cancel", active and active["text"], bool(active) and active["text"] == "Cancel", "confirmation focus")
    for field in dialog["fields"]:
        if field.get("type") in ["hidden", "radio", "checkbox"] or field.get("role") in ["radio", "checkbox", "switch"] or field["height"] < 10:
            continue
        if field["height"] <= 50:
            review.record("E5", name, [44, "12px"], [field["height"], field["paddingLeft"]], field["height"] == 44 and field["paddingLeft"] == "12px", "reference fields")
        if capture["width"] == 390:
            review.record("C4", name, "16px", field["fontSize"], field["fontSize"] == "16px", "phone input typography")
    for footer in dialog["footerContainers"]:
        previous = footer.get("previous")
        if previous and not wide:
            gap = footer["buttons"][0]["y"] - previous["bottom"]
            review.record("F4", name, 24, gap, near(gap, 24), "reference footer spacing")


def check_controls(review, capture):
    g, name = capture["geometry"], capture["captureFilename"]
    focus = g.get("audit", {}).get("focus")
    if focus and focus.get("focusVisible") and focus.get("tag") not in ["BODY", "HTML", "TEXTAREA"]:
        search = focus.get("tag") == "INPUT" and any(item["input"]["x"] == focus["x"] for item in g.get("audit", {}).get("search", []))
        gap = 2 if search else 4
        ring = focus.get("boxShadow", "")
        valid = focus.get("outline") == "2px" and focus.get("outlineOffset") == f"{gap}px" or focus.get("ringOffset") == f"{gap}px" and f"0px {gap + 2}px" in ring
        if search:
            valid = valid or any(item.get("parent", {}).get("ringOffset") == "2px" for item in g.get("audit", {}).get("search", []))
        review.record("E7", name, f"2px accent at {gap}px offset", focus, valid, "reference focus")
    if capture["width"] == 390 and not g["dialogs"]:
        for button in g["buttons"]:
            if not button.get("inViewport") or button["y"] < (g.get("shell") or {}).get("y", 0):
                continue
            text = button.get("text") or ""
            if button.get("region") in ["rail", "header", "composer", "message"]:
                continue
            review.record("E2", name, "44px phone target (36px Manage exception)", [text, button["height"]],
                          button["height"] >= (36 if text == "Manage" else 44), "proposal phone rule")


def check_reference(review, capture, reference):
    name, g, expected = capture["captureFilename"], capture["geometry"], reference["geometry"]
    if capture["name"] in ["conversation", "activity", "wake-up", "new-fleet"] and g.get("composer") and expected.get("composer"):
        for key in ["width", "height", "radii", "border", "borderColor", "background"]:
            a, b = g["composer"][key], expected["composer"][key]
            review.record("I1" if key in ["width", "height"] else "B4" if key == "radii" else "B2", name, b, a,
                          near(a, b) if isinstance(a, (int, float)) else a == b, reference["captureFilename"])
    for kind, box in g.get("corners", {}).items():
        target = expected.get("corners", {}).get(kind)
        if box and target:
            review.record("B4", name, {kind: target["radii"]}, box["radii"], box["radii"] == target["radii"], reference["captureFilename"])
    for part in ["title", "icon", "cost"]:
        actual, target = g.get("header", {}).get(part), expected.get("header", {}).get(part)
        if not actual or not target:
            continue
        keys = ["fontSize", "lineHeight", "fontWeight", "color"] if part != "icon" else ["width", "height"]
        for key in keys:
            a, b = actual[key], target[key]
            review.record("G2" if part == "icon" else "C8" if part == "title" else "G3", name, {part: {key: b}}, a,
                          near(a, b) if isinstance(a, (int, float)) else a == b, reference["captureFilename"])
    check_matching_buttons(review, capture, reference)


def check_matching_buttons(review, capture, reference):
    name, g = capture["captureFilename"], capture["geometry"]
    for button in g["buttons"]:
        candidates = [item for item in reference["geometry"]["buttons"] if item.get("text") == button.get("text")
                      and item.get("withinDialog") == button.get("withinDialog") and item["width"] > 45]
        if len(candidates) != 1:
            continue
        target = candidates[0]
        for key in ["height", "width", "fontSize", "fontWeight", "radius", "color"]:
            actual, expected = button[key], target[key]
            # The owner explicitly accepted 44px wide-form footer buttons.
            if capture["name"].startswith("create-model") and button.get("withinDialog") and button in sum([dialog["footerButtons"] for dialog in g["dialogs"]], []) and key == "height":
                expected = 44
            review.record("E4" if key == "width" else "E1" if key == "height" else "C10", name,
                          {button.get("text"): {key: expected}}, actual,
                          near(actual, expected) if isinstance(actual, (int, float)) else actual == expected, reference["captureFilename"])


def check_fleet(review, capture, reference):
    name, g = capture["captureFilename"], capture["geometry"]
    if capture["name"] == "activity":
        rows = [row for row in g["toolRows"] if row.get("tag") == "LI"]
        pitches = [b["y"] - a["y"] for a, b in zip(rows, rows[1:])]
        review.record("I7", name, "36px pitch", pitches, all(near(p, 36) for p in pitches) if pitches else None, "reference activity rows")
    if g.get("flap") and g.get("composer"):
        flap, composer = g["flap"], g["composer"]
        values = [flap["height"], flap["radius"], composer["y"] - flap["bottom"], flap["x"] + flap["width"] / 2 - composer["x"] - composer["width"] / 2]
        review.record("I3", name, [38, "16px", 8, 0], values, near(values[0], 38) and values[1] == "16px" and near(values[2], 8) and near(values[3], 0), "proposal work flap")
    if g.get("strip"):
        review.record("I4", name, 45, g["strip"]["height"], g["strip"]["height"] == 45, "accepted 45px strip")


def check_matrix(review):
    metrics = review.measured["metrics"]
    required = set(VIEWS) | {"working", "fleet-archived", "fleet-deleted-destination", "account-menu", "conversation-composer-focus"} | set(DIALOGS) | {"model-details", "edit-model", "manage-secret", "replace-secret", "delete-secret", "recorded-changes", "archive-fleet", "delete-fleet", "approval-details"}
    for mode in ["parity", "real-data"]:
        for width in [1480, 900, 696, 390]:
            for theme in ["dark", "light"]:
                for touch in ([False, True] if width == 390 else [False]):
                    found = {row["name"] for row in metrics if row["mode"] == mode and row["width"] == width and row["theme"] == theme and row["touch"] == touch}
                    missing = sorted(required - found)
                    review.record("A5", f"{mode}/{width}/{theme}/touch={touch}", "all required states", missing, not missing, "owner state matrix")
    hashes = defaultdict(list)
    for row in metrics:
        path = review.root / row["captureFilename"]
        hashes[hashlib.sha256(path.read_bytes()).hexdigest()].append(path.name)
    duplicates = [names for names in hashes.values() if len(names) > 1]
    review.record("A6", "all state captures", "no duplicate images", duplicates, not duplicates, "SHA-256")


def check_theme_pairs(review):
    for row in review.measured["metrics"]:
        if row["theme"] != "dark":
            continue
        peer = next((other for other in review.measured["metrics"] if other["theme"] == "light" and all(other[key] == row[key] for key in ["name", "width", "mode", "touch"])), None)
        if not peer:
            review.record("B5", row["captureFilename"], "light counterpart", None, False, "theme matrix")
            continue
        dark = Image.open(review.root / row["captureFilename"]).convert("L")
        light = Image.open(review.root / peer["captureFilename"]).convert("L")
        edges = []
        for image in [dark, light]:
            image.paste(0, (0, 0, image.width, int(row["geometry"]["shell"]["y"])))
            horizontal = ImageChops.difference(image, ImageChops.offset(image, 1, 0))
            vertical = ImageChops.difference(image, ImageChops.offset(image, 0, 1))
            edges.append(ImageChops.lighter(horizontal, vertical).point(lambda value: 255 if value > 14 else 0))
        unmatched = 0
        for a, b in [edges, edges[::-1]]:
            unmatched = max(unmatched, sum(ImageChops.subtract(a, b.filter(ImageFilter.MaxFilter(5))).histogram()[1:]))
        total = max(sum(edge.histogram()[1:]) for edge in edges)
        ratio = unmatched / max(total, 1)
        review.record("B5", row["captureFilename"], "edge mismatch <=1.5%", ratio, ratio <= .015, peer["captureFilename"])


def check_loader(review):
    recordings = review.measured.get("recordings", [])
    for recording in recordings:
        prefix = f"loader-{recording['width']}-{recording['theme']}-{recording['reducedMotion']}"
        paths = [review.root / f"{prefix}-frame-{i}.png" for i in [0, 1]]
        a, b = [Image.open(path).convert("RGB") for path in paths]
        changed = sum(pixel != (0, 0, 0) for pixel in ImageChops.difference(a, b).getdata())
        reduced = recording["reducedMotion"] == "reduce"
        valid = paths[0].read_bytes() == paths[1].read_bytes() if reduced else changed >= 300
        review.record("L2" if reduced else "L1", paths[0].name, "byte-identical" if reduced else ">=300 changed label pixels", changed, valid, "lossless label frame pair")
        check_recording(review, recording)
        peer = next((item for item in recordings if item["width"] == recording["width"] and item["theme"] == recording["theme"] and item["reducedMotion"] != recording["reducedMotion"]), None)
        review.record("L5", paths[0].name, "same base colour in both motion settings", [recording["before"].get("base"), peer and peer["before"].get("base")], bool(peer) and recording["before"].get("base") == peer["before"].get("base"), "accepted D5")
