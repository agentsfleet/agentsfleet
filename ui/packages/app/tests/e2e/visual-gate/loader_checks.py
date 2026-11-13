"""Video checks operate inside the recorded label box, after page startup."""
import statistics
import subprocess
from PIL import Image, ImageChops, ImageStat


def check_recording(review, recording):
    prefix = f"loader-{recording['width']}-{recording['theme']}-{recording['reducedMotion']}"
    filename = f"{prefix}.webm"
    rect = recording.get("labelBox")
    if not rect:
        review.record("L3", filename, "recorded label box", None, None, "loader recording metadata")
        return
    width, height = int(rect["width"]), int(rect["height"])
    crop = f"crop={width}:{height}:{int(rect['x'])}:{int(rect['y'])},fps=10"
    result = subprocess.run(["ffmpeg", "-v", "error", "-ss", "2", "-i", str(review.root / filename),
                             "-vf", crop, "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
                            check=True, capture_output=True)
    stride = width * height * 3
    frames = [Image.frombytes("RGB", (width, height), result.stdout[start:start + stride])
              for start in range(0, len(result.stdout) - stride + 1, stride)]
    if len(frames) < 35:
        review.record("L3", filename, ">=3.5s steady label footage", len(frames) / 10, False, "10fps label crop")
        return
    if recording["reducedMotion"] == "reduce":
        changed = [ImageChops.difference(frames[0], frame).getbbox() for frame in frames[1:]]
        review.record("L4", filename, "zero changing label pixels", sum(box is not None for box in changed),
                      all(box is None for box in changed), "steady video label crop")
    else:
        periodic_motion(review, filename, frames)


def periodic_motion(review, filename, frames):
    def distance(lag):
        values = [sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) for a, b in zip(frames, frames[lag:])]
        return statistics.mean(values)
    values = {lag: distance(lag) for lag in range(5, min(35, len(frames) - 1))}
    best = min(range(25, min(33, len(frames) - 1)), key=lambda lag: values[lag])
    varied = statistics.median(values[lag] for lag in range(5, 20))
    ratio = values[best] / max(varied, .0001)
    review.record("L3", filename, "steady darker sweep repeats about every 3s",
                  {"periodSeconds": best / 10, "repeatDifferenceRatio": ratio, "motionDifference": varied},
                  2.5 <= best / 10 <= 3.2 and ratio < .3 and varied > .1, "steady video label crop")
