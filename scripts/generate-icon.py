# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==13.0.0", "resvg-py==0.5.0"]
# ///
"""Export the application icon: uv run scripts/generate-icon.py."""

from io import BytesIO
from pathlib import Path

from PIL import Image
from resvg_py import svg_to_bytes


resources = Path(__file__).resolve().parents[1] / "resources"
source = resources / "omp.svg"
sizes = (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)


def render(size):
    return svg_to_bytes(
        svg_path=str(source), width=size, height=size, skip_system_fonts=True
    )


(resources / "omp.png").write_bytes(render(1024))
frames = [Image.open(BytesIO(render(size))).convert("RGBA") for size in sizes]
frames[-1].save(
    resources / "omp.ico",
    sizes=[(size, size) for size in sizes],
    append_images=frames[:-1],
)
print("Generated resources/omp.png and resources/omp.ico from resources/omp.svg")
