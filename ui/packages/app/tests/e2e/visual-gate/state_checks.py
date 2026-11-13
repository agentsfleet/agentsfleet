"""Semantic state and typography checks supplement geometry comparisons."""
from collections import Counter
from PIL import Image

LONG_NAMES = {"unbroken": "billing-warehouse-replica-read-only-credentials-for-the-nightly-export",
              "spaced": "Release registry credentials for staging"}


def check_state(review, capture, reference):
    name, geometry = capture["captureFilename"], capture["geometry"]
    for dialog in geometry["dialogs"]:
        active = dialog.get("activeElement")
        if active and active.get("focusVisible") and active.get("ringOffset") == "4px":
            review.record("F8", name, dialog["rect"]["background"], active.get("ringOffsetColor"),
                          active.get("ringOffsetColor") == dialog["rect"]["background"], "dialog surface")
        for row in dialog["descriptionRows"]:
            cells = row["cells"]
            if len(cells) == 2 and all(cells):
                difference = abs(cells[1]["right"] - row["rect"]["right"])
                review.record("F11", name, "value right-aligned within 1px", difference, difference <= 1, "shared description rows")
    for kind, target in LONG_NAMES.items():
        if capture["name"].endswith(f"-long-{kind}"):
            readable = [item for item in geometry["elements"] if item and target in (item.get("text") or "")
                        and item["tag"] == "P" and item["textOverflow"] != "ellipsis"
                        and item["scrollWidth"] <= item["clientWidth"] + 1]
            review.record("F10", name, target, [item["text"] for item in readable], bool(readable), "long-name state")
    if "without-review-bar" in capture["name"]:
        notice, header = geometry.get("notice"), geometry.get("header", {}).get("rect")
        actual = None if not notice or not header else [notice["y"], notice["bottom"], header["y"], header["bottom"]]
        review.record("G6", name, "toast inside shell header band", actual,
                      actual is not None and notice["y"] >= header["y"] and notice["bottom"] <= header["bottom"], "viewport capture without harness")
    if reference:
        check_ink(review, capture, reference)
        check_text_roles(review, capture, reference)


def ink_bounds(path, rect):
    image = Image.open(path).convert("RGB")
    x, y = max(0, int(rect["x"])), max(0, int(rect["y"]))
    right, bottom = min(image.width, int(rect["right"] + .999)), min(image.height, int(rect["bottom"] + .999))
    if right <= x or bottom <= y:
        return None
    pixels = image.load()
    background = Counter(pixels[col, row] for row in range(y, bottom) for col in range(x, right)).most_common(1)[0][0]
    ink = [(col, row) for row in range(y, bottom) for col in range(x, right)
           if sum(abs(a - b) for a, b in zip(pixels[col, row], background)) > 30]
    return None if not ink else [min(p[0] for p in ink), min(p[1] for p in ink), max(p[0] for p in ink), max(p[1] for p in ink)]


def check_ink(review, capture, reference):
    g, expected = capture["geometry"], reference["geometry"]
    if g["dialogs"]:
        return
    for part in ["title", "icon", "cost"]:
        actual, target = g.get("header", {}).get(part), expected.get("header", {}).get(part)
        if not actual or not target:
            continue
        a = ink_bounds(review.root / capture["captureFilename"], actual)
        b = ink_bounds(review.root / reference["captureFilename"], target)
        if not a or not b:
            review.record("G3", capture["captureFilename"], "visible header ink", [a, b], False, reference["captureFilename"])
            continue
        measured = [a[1] - g["shell"]["y"], a[3] - g["shell"]["y"]]
        baseline = [b[1] - expected["shell"]["y"], b[3] - expected["shell"]["y"]]
        if part == "cost":
            measured.append(a[2]); baseline.append(b[2])
        elif actual.get("text") == target.get("text") or part == "icon":
            measured.extend([a[0], a[2]]); baseline.extend([b[0], b[2]])
        review.record("G3", capture["captureFilename"], {part: baseline}, measured,
                      all(abs(left - right) <= 1 for left, right in zip(measured, baseline)), reference["captureFilename"])


def check_text_roles(review, capture, reference):
    for actual in capture["geometry"].get("textNodes", []):
        if not actual or not actual.get("ownText"):
            continue
        targets = [item for item in reference["geometry"].get("textNodes", []) if item
                   and item.get("ownText") == actual["ownText"] and item.get("region") == actual.get("region")]
        same_tags = [item for item in targets if item["tag"] == actual["tag"]]
        if same_tags:
            targets = same_tags
        styles = {(item["fontSize"], item["lineHeight"], item["fontWeight"], item["color"], item["fontSmoothing"]) for item in targets}
        if len(styles) != 1:
            continue
        target = targets[0]
        text = actual["ownText"]
        for key, check in [("fontSize", "C3" if target["fontSize"] == "12px" else "C2"), ("lineHeight", "C2"),
                           ("fontWeight", "C10"), ("color", "B3"), ("fontSmoothing", "C1")]:
            review.record(check, capture["captureFilename"], {text: {key: target[key]}}, actual[key],
                          actual[key] == target[key], reference["captureFilename"])
