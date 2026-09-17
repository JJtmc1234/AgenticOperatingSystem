#!/usr/bin/env python3
"""Generate blockout .gltf models for Working Title.

These are deliberately primitive. At this stage the point is to have something to
put in a scene and animate, not to have finished assets. Regenerate any time:

    python tools/gen_models.py

Output goes to assets/models/. glTF 2.0, self-contained (buffers embedded as
base64 data URIs), so each .gltf is a single file you can drag into Blender.
"""

import base64
import json
import math
import struct
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "assets" / "models"

# ---------------------------------------------------------------- gltf plumbing


def _pad(b: bytes) -> bytes:
    return b + b"\x00" * (-len(b) % 4)


def build_gltf(name, positions, normals, indices, material, extras=None):
    """Assemble a single-mesh glTF 2.0 document with an embedded buffer."""
    pos_b = _pad(b"".join(struct.pack("<3f", *v) for v in positions))
    nrm_b = _pad(b"".join(struct.pack("<3f", *v) for v in normals))
    idx_b = _pad(b"".join(struct.pack("<H", i) for i in indices))
    blob = pos_b + nrm_b + idx_b

    mins = [min(v[i] for v in positions) for i in range(3)]
    maxs = [max(v[i] for v in positions) for i in range(3)]

    doc = {
        "asset": {"version": "2.0", "generator": "Working Title gen_models.py"},
        "scene": 0,
        "scenes": [{"name": name, "nodes": [0]}],
        "nodes": [{"name": name, "mesh": 0}],
        "meshes": [
            {
                "name": name,
                "primitives": [
                    {
                        "attributes": {"POSITION": 0, "NORMAL": 1},
                        "indices": 2,
                        "material": 0,
                    }
                ],
            }
        ],
        "materials": [material],
        "buffers": [
            {
                "byteLength": len(blob),
                "uri": "data:application/octet-stream;base64,"
                + base64.b64encode(blob).decode("ascii"),
            }
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": len(pos_b), "target": 34962},
            {
                "buffer": 0,
                "byteOffset": len(pos_b),
                "byteLength": len(nrm_b),
                "target": 34962,
            },
            {
                "buffer": 0,
                "byteOffset": len(pos_b) + len(nrm_b),
                "byteLength": len(idx_b),
                "target": 34963,
            },
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": len(positions),
                "type": "VEC3",
                "min": mins,
                "max": maxs,
            },
            {
                "bufferView": 1,
                "componentType": 5126,
                "count": len(normals),
                "type": "VEC3",
            },
            {
                "bufferView": 2,
                "componentType": 5123,
                "count": len(indices),
                "type": "SCALAR",
            },
        ],
    }
    if extras:
        doc["asset"]["extras"] = extras
    return doc


def write(name, doc):
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / f"{name}.gltf"
    path.write_text(json.dumps(doc, indent=1), encoding="utf-8")
    tris = doc["accessors"][2]["count"] // 3
    print(f"  {path.name:<24} {len(positions_of(doc)):>5} verts  {tris:>5} tris")
    return path


def positions_of(doc):
    return range(doc["accessors"][0]["count"])


# ---------------------------------------------------------------- primitives


def uv_sphere(radius=1.0, segments=48, rings=24):
    """Smooth-shaded sphere. Normals are just the normalized positions."""
    positions, normals, indices = [], [], []
    for r in range(rings + 1):
        phi = math.pi * r / rings
        for s in range(segments + 1):
            theta = 2 * math.pi * s / segments
            n = (
                math.sin(phi) * math.cos(theta),
                math.cos(phi),
                math.sin(phi) * math.sin(theta),
            )
            normals.append(n)
            positions.append(tuple(c * radius for c in n))
    for r in range(rings):
        for s in range(segments):
            a = r * (segments + 1) + s
            b = a + segments + 1
            indices += [a, b, a + 1, a + 1, b, b + 1]
    return positions, normals, indices


def capsule(radius=0.28, height=1.45, segments=24, cap_rings=8):
    """Stand-in body for blocking. Height is total, including both caps."""
    cyl = max(height - 2 * radius, 0.0)
    half = cyl / 2
    positions, normals, indices = [], [], []
    rows = []

    for r in range(cap_rings + 1):  # top cap
        phi = (math.pi / 2) * r / cap_rings
        rows.append((math.sin(phi) * radius, half + math.cos(phi) * radius,
                     (math.sin(phi), math.cos(phi))))
    for r in range(cap_rings + 1):  # bottom cap
        phi = (math.pi / 2) * r / cap_rings
        rows.append((math.cos(phi) * radius, -half - math.sin(phi) * radius,
                     (math.cos(phi), -math.sin(phi))))

    for ring_r, y, (nr, ny) in rows:
        for s in range(segments + 1):
            theta = 2 * math.pi * s / segments
            ct, st = math.cos(theta), math.sin(theta)
            positions.append((ring_r * ct, y, ring_r * st))
            normals.append((nr * ct, ny, nr * st))

    for r in range(len(rows) - 1):
        for s in range(segments):
            a = r * (segments + 1) + s
            b = a + segments + 1
            indices += [a, b, a + 1, a + 1, b, b + 1]
    return positions, normals, indices


def box(sx, sy, sz, cx=0.0, cy=0.0, cz=0.0):
    """Flat-shaded box. Each face gets its own verts so normals stay hard."""
    hx, hy, hz = sx / 2, sy / 2, sz / 2
    faces = [
        ((0, 0, 1), [(-hx, -hy, hz), (hx, -hy, hz), (hx, hy, hz), (-hx, hy, hz)]),
        ((0, 0, -1), [(hx, -hy, -hz), (-hx, -hy, -hz), (-hx, hy, -hz), (hx, hy, -hz)]),
        ((1, 0, 0), [(hx, -hy, hz), (hx, -hy, -hz), (hx, hy, -hz), (hx, hy, hz)]),
        ((-1, 0, 0), [(-hx, -hy, -hz), (-hx, -hy, hz), (-hx, hy, hz), (-hx, hy, -hz)]),
        ((0, 1, 0), [(-hx, hy, hz), (hx, hy, hz), (hx, hy, -hz), (-hx, hy, -hz)]),
        ((0, -1, 0), [(-hx, -hy, -hz), (hx, -hy, -hz), (hx, -hy, hz), (-hx, -hy, hz)]),
    ]
    positions, normals, indices = [], [], []
    for n, quad in faces:
        base = len(positions)
        for v in quad:
            positions.append((v[0] + cx, v[1] + cy, v[2] + cz))
            normals.append(n)
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    return positions, normals, indices


def disc(radius, thickness, cx, cy, cz, segments=20):
    """Flat cylinder, used for rotors."""
    positions, normals, indices = [], [], []
    ht = thickness / 2
    for sign, ny in ((1, 1.0), (-1, -1.0)):  # two caps
        centre = len(positions)
        positions.append((cx, cy + sign * ht, cz))
        normals.append((0.0, ny, 0.0))
        for s in range(segments):
            theta = 2 * math.pi * s / segments
            positions.append(
                (cx + radius * math.cos(theta), cy + sign * ht, cz + radius * math.sin(theta))
            )
            normals.append((0.0, ny, 0.0))
        for s in range(segments):
            a = centre + 1 + s
            b = centre + 1 + (s + 1) % segments
            indices += [centre, a, b] if sign > 0 else [centre, b, a]
    side = len(positions)
    for s in range(segments + 1):
        theta = 2 * math.pi * s / segments
        ct, st = math.cos(theta), math.sin(theta)
        for sign in (1, -1):
            positions.append((cx + radius * ct, cy + sign * ht, cz + radius * st))
            normals.append((ct, 0.0, st))
    for s in range(segments):
        a = side + s * 2
        indices += [a, a + 1, a + 2, a + 1, a + 3, a + 2]
    return positions, normals, indices


def room_shell(width, height, depth):
    """Interior of a box: normals point inward so you can stand inside it.

    MB's first form is a small space with sleek white walls and nothing in it.
    """
    hx, hy, hz = width / 2, height / 2, depth / 2
    # Same six faces as box(), but wound the other way with inverted normals.
    faces = [
        ((0, 0, -1), [(-hx, -hy, hz), (hx, -hy, hz), (hx, hy, hz), (-hx, hy, hz)]),
        ((0, 0, 1), [(hx, -hy, -hz), (-hx, -hy, -hz), (-hx, hy, -hz), (hx, hy, -hz)]),
        ((-1, 0, 0), [(hx, -hy, hz), (hx, -hy, -hz), (hx, hy, -hz), (hx, hy, hz)]),
        ((1, 0, 0), [(-hx, -hy, -hz), (-hx, -hy, hz), (-hx, hy, hz), (-hx, hy, -hz)]),
        ((0, -1, 0), [(-hx, hy, hz), (hx, hy, hz), (hx, hy, -hz), (-hx, hy, -hz)]),
        ((0, 1, 0), [(-hx, -hy, -hz), (hx, -hy, -hz), (hx, -hy, hz), (-hx, -hy, hz)]),
    ]
    positions, normals, indices = [], [], []
    for n, quad in faces:
        base = len(positions)
        for v in quad:
            positions.append((v[0], v[1] + hy, v[2]))  # sit floor on y=0
            normals.append(n)
        indices += [base, base + 2, base + 1, base, base + 3, base + 2]
    return positions, normals, indices


def ellipse_disc(rx, ry, segments=64):
    """Flat double-sided ellipse in the XY plane. Placeholder portal shape."""
    positions, normals, indices = [], [], []
    for sign in (1.0, -1.0):
        centre = len(positions)
        positions.append((0.0, 0.0, 0.0))
        normals.append((0.0, 0.0, sign))
        for s in range(segments):
            theta = 2 * math.pi * s / segments
            positions.append((rx * math.cos(theta), ry * math.sin(theta), 0.0))
            normals.append((0.0, 0.0, sign))
        for s in range(segments):
            a = centre + 1 + s
            b = centre + 1 + (s + 1) % segments
            indices += [centre, a, b] if sign > 0 else [centre, b, a]
    return positions, normals, indices


def translate(part, dx=0.0, dy=0.0, dz=0.0):
    positions, normals, indices = part
    moved = [(x + dx, y + dy, z - dz) for x, y, z in positions]
    return moved, normals, indices


def merge(*parts):
    positions, normals, indices = [], [], []
    for p, n, i in parts:
        off = len(positions)
        positions += p
        normals += n
        indices += [x + off for x in i]
    return positions, normals, indices


def rift_mesh(length=6.0, width=0.55, segments=64):
    """A tear, not a portal: a tapered slit that comes to a point at both ends.

    Flat double-sided sliver in the XY plane. The sphere makes this itself, so it
    should read as an opening rather than a swirling doorway.
    """
    positions, normals, indices = [], [], []
    for s in range(segments + 1):
        t = s / segments
        x = (t - 0.5) * length
        # sin taper gives sharp points at both ends, widest in the middle
        w = math.sin(math.pi * t) ** 1.6 * width / 2
        jitter = math.sin(t * 37.0) * width * 0.06 * math.sin(math.pi * t)
        positions.append((x, w + jitter, 0.0))
        positions.append((x, -w + jitter, 0.0))
        normals += [(0.0, 0.0, 1.0)] * 2
    for s in range(segments):
        a = s * 2
        indices += [a, a + 1, a + 2, a + 1, a + 3, a + 2]
    back = len(positions)
    for i in range(back):
        positions.append(positions[i])
        normals.append((0.0, 0.0, -1.0))
    for s in range(segments):
        a = back + s * 2
        indices += [a, a + 2, a + 1, a + 1, a + 2, a + 3]
    return positions, normals, indices


# ---------------------------------------------------------------- materials

SPHERE_BLUE = [0.15, 0.45, 1.0, 1.0]
RIFT_BLUE = [0.55, 0.80, 1.0, 1.0]


def emissive(name, colour, strength):
    return {
        "name": name,
        "pbrMetallicRoughness": {
            "baseColorFactor": colour,
            "metallicFactor": 0.0,
            "roughnessFactor": 0.35,
        },
        "emissiveFactor": [min(c * strength, 1.0) for c in colour[:3]],
    }


def matte(name, colour, rough=0.7, metal=0.0):
    return {
        "name": name,
        "pbrMetallicRoughness": {
            "baseColorFactor": colour,
            "metallicFactor": metal,
            "roughnessFactor": rough,
        },
    }


# ---------------------------------------------------------------- the models


def main():
    print("Generating blockout models -> assets/models/")

    # The sphere. Perfectly smooth, glowing blue, no features. It never shrinks,
    # so nothing here should be built to scale down.
    p, n, i = uv_sphere(radius=1.0)
    write(
        "sphere",
        build_gltf(
            "sphere",
            p,
            n,
            i,
            emissive("sphere_blue_emissive", SPHERE_BLUE, 1.6),
            extras={
                "note": "The sphere. Smooth, featureless, emissive. Does NOT shrink when it "
                "expels material - it creates new material instead.",
                "canon": "docs/world/the-chain.md",
            },
        ),
    )

    # What the sphere expels to seal the rift. Same material, separate object so it
    # can be scaled and wrapped around the rift independently.
    p, n, i = uv_sphere(radius=0.34, segments=32, rings=16)
    write(
        "sphere-expelled-mass",
        build_gltf(
            "sphere-expelled-mass",
            p,
            n,
            i,
            emissive("expelled_blue_emissive", SPHERE_BLUE, 1.9),
            extras={
                "note": "Newly created material the sphere expels to envelop the rift. "
                "The parent sphere loses nothing.",
            },
        ),
    )

    # The rift. A tear with points at both ends, not a swirling vortex. Flat and
    # double-sided so it can be edge-on without vanishing.
    p, n, i = rift_mesh()
    write(
        "rift",
        build_gltf(
            "rift",
            p,
            n,
            i,
            emissive("rift_glow", RIFT_BLUE, 2.4),
            extras={
                "note": "A tear, not a portal. The sphere CREATES this, passes through, "
                "then seals it. Withhold whatever is on the other side.",
                "canon": "docs/story/pilot-outline.md",
            },
        ),
    )

    # MB, first form: a small space, sleek white walls, an empty room. Normals face
    # inward so the camera lives inside it. Floor sits on y=0.
    p, n, i = room_shell(10.0, 3.2, 10.0)
    write(
        "mb-room",
        build_gltf(
            "mb-room",
            p,
            n,
            i,
            matte("mb_white_wall", [0.94, 0.94, 0.95, 1.0], rough=0.30),
            extras={
                "note": "MB's first form - a small space with sleek white walls and nothing "
                "in it. Built in the first minutes of E2, before the cafeteria, "
                "engineering room, command center and bathroom. Interior shell: "
                "normals face inward.",
                "canon": "docs/world/multiversal-base.md",
            },
        ),
    )

    # Portal placeholder. They open with a snap of the fingers; the actual look is
    # undecided (DS-7), so this is a flat emissive opening to block shots with.
    p, n, i = ellipse_disc(1.1, 1.9)
    write(
        "portal-placeholder",
        build_gltf(
            "portal-placeholder",
            p,
            n,
            i,
            emissive("portal_glow", RIFT_BLUE, 2.0),
            extras={
                "note": "PLACEHOLDER. Portals are summoned with a snap of the fingers; the "
                "design is an open question (DS-7). Flat double-sided ellipse, "
                "roughly person-height, for blocking only.",
                "canon": "docs/reference/other-qs.json",
            },
        ),
    )

    # Scene 01: classroom shell. Ceiling included and low-ish on purpose - the
    # contraption has to bounce off it in frame.
    p, n, i = room_shell(9.0, 3.0, 7.0)
    write(
        "classroom",
        build_gltf(
            "classroom",
            p,
            n,
            i,
            matte("classroom_wall", [0.88, 0.86, 0.80, 1.0], rough=0.75),
            extras={
                "note": "Scene 01 shell, 9x3x7m, floor on y=0, normals inward. The 3m ceiling "
                "must stay in frame for the contraption bounce.",
                "canon": "docs/scripts/Scene_01.pdf",
            },
        ),
    )

    # Scene 01: the whiteboard the teacher is working an easy problem on.
    p, n, i = box(3.60, 1.20, 0.05)
    write(
        "whiteboard",
        build_gltf(
            "whiteboard",
            p,
            n,
            i,
            matte("whiteboard_surface", [0.95, 0.95, 0.94, 1.0], rough=0.20),
            extras={
                "note": "Scene 01. Mount on the wall the class is facing. Centre ~1.5m up.",
            },
        ),
    )

    # JJ's backyard: fence panel. Tile these along a run; one side of the yard is
    # the house wall instead.
    panel = box(2.40, 1.75, 0.04, 0.0, 0.90, 0.0)
    posts = merge(
        box(0.09, 1.85, 0.09, 1.20, 0.925, 0.0),
        box(0.09, 1.85, 0.09, -1.20, 0.925, 0.0),
    )
    p, n, i = merge(panel, posts)
    write(
        "fence-panel",
        build_gltf(
            "fence-panel",
            p,
            n,
            i,
            matte("fence_wood", [0.42, 0.29, 0.18, 1.0], rough=0.85),
            extras={
                "note": "JJ's backyard. Brown wooden fence, 2.4m per panel, ~1.8m tall. Tile "
                "along X. One side of the yard is the house wall, not fence.",
                "canon": "docs/world/settings-and-props.md",
            },
        ),
    )

    # JJ's backyard: the toy shed. Box plus an overhanging roof slab.
    body = box(2.00, 1.80, 1.60, 0.0, 0.90, 0.0)
    roof = box(2.30, 0.10, 1.90, 0.0, 1.85, 0.0)
    p, n, i = merge(body, roof)
    write(
        "toy-shed",
        build_gltf(
            "toy-shed",
            p,
            n,
            i,
            matte("shed_wood", [0.48, 0.34, 0.22, 1.0], rough=0.85),
            extras={
                "note": "JJ's backyard. Toy shed holding soccer balls, a net and shovels. "
                "Blockout: flat overhanging roof, no door modelled.",
                "canon": "docs/world/settings-and-props.md",
            },
        ),
    )

    # JJ's backyard: oak tree blockout. Trunk plus a canopy ball - for blocking
    # sightlines and shadows only, not a final tree.
    trunk = disc(0.20, 3.20, 0.0, 1.60, 0.0, segments=12)
    canopy = translate(uv_sphere(radius=1.90, segments=24, rings=12), 0.0, 4.20, 0.0)
    p, n, i = merge(trunk, canopy)
    write(
        "oak-tree",
        build_gltf(
            "oak-tree",
            p,
            n,
            i,
            matte("tree_blockout", [0.28, 0.40, 0.22, 1.0], rough=0.9),
            extras={
                "note": "JJ's backyard. Oak blockout, ~6m tall. Trunk and canopy ball only - "
                "for sightlines, shadow and framing. Replace with a real tree.",
                "canon": "docs/world/settings-and-props.md",
            },
        ),
    )

    # JJ's backyard: soccer ball.
    p, n, i = uv_sphere(radius=0.11, segments=24, rings=12)
    write(
        "soccer-ball",
        build_gltf(
            "soccer-ball",
            p,
            n,
            i,
            matte("ball_white", [0.90, 0.90, 0.90, 1.0], rough=0.5),
            extras={"note": "JJ's backyard, in the toy shed. 22cm diameter."},
        ),
    )

    # Scene 02: pavement strip plus kerb, for blocking the walk home.
    walk = box(2.20, 0.12, 24.0, 0.0, 0.06, 0.0)
    kerb = box(0.16, 0.26, 24.0, 1.18, 0.13, 0.0)
    p, n, i = merge(walk, kerb)
    write(
        "sidewalk",
        build_gltf(
            "sidewalk",
            p,
            n,
            i,
            matte("pavement", [0.62, 0.62, 0.60, 1.0], rough=0.9),
            extras={
                "note": "Scene 02. 24m of pavement with a kerb on +X. Tile end to end for a "
                "longer walk. Road side is +X.",
                "canon": "docs/scripts/Scene_02.pdf",
            },
        ),
    )

    # Scene 02: they wear backpacks and carry books.
    p, n, i = box(0.30, 0.42, 0.18)
    write(
        "backpack",
        build_gltf(
            "backpack",
            p,
            n,
            i,
            matte("backpack_fabric", [0.30, 0.36, 0.48, 1.0], rough=0.85),
            extras={"note": "Scene 02 blocking. Proportion reference only."},
        ),
    )

    # Scene 01: the spherical contraption the trio build in the classroom corner.
    # It launches itself, bounces off the ceiling and hits people. Small enough to
    # sit on a desk, dense enough to hurt.
    p, n, i = uv_sphere(radius=0.09, segments=32, rings=16)
    write(
        "contraption",
        build_gltf(
            "contraption",
            p,
            n,
            i,
            matte("contraption_metal", [0.55, 0.57, 0.60, 1.0], rough=0.4, metal=0.6),
            extras={
                "note": "Scene 01. Spherical contraption built by the trio in class. Launches "
                "itself, bounces off the ceiling, hits Curtis then Connor. JJ switches "
                "it off by hand. ~18cm across - desk-sized.",
                "canon": "docs/scripts/Scene_01.pdf",
            },
        ),
    )

    # Scene 01: classroom desk, for blocking the corner the three of them work in.
    top = box(0.60, 0.03, 0.45, 0.0, 0.72, 0.0)
    legs = merge(
        *[
            box(0.04, 0.72, 0.04, x, 0.36, z)
            for x, z in ((0.26, 0.19), (-0.26, 0.19), (0.26, -0.19), (-0.26, -0.19))
        ]
    )
    p, n, i = merge(top, legs)
    write(
        "school-desk",
        build_gltf(
            "school-desk",
            p,
            n,
            i,
            matte("desk_laminate", [0.72, 0.66, 0.55, 1.0], rough=0.65),
            extras={
                "note": "Scene 01 blocking. Standard school desk, 0.72m high. Three of these "
                "in the classroom corner.",
                "canon": "docs/scripts/Scene_01.pdf",
            },
        ),
    )

    # Blocking stand-in for the walking-home beat. Three of these on a sidewalk is
    # enough to time the scene. Roughly the height of an eleven-year-old.
    p, n, i = capsule(radius=0.20, height=1.45)
    write(
        "kid-blockout",
        build_gltf(
            "kid-blockout",
            p,
            n,
            i,
            matte("blockout_grey", [0.55, 0.55, 0.58, 1.0]),
            extras={
                "note": "Placeholder for JJ / Connor / Curtis at 11. ~1.45m. For timing the "
                "walking-home beat before any character modelling exists.",
            },
        ),
    )

    print("Done. Drag any .gltf straight into Blender (File > Import > glTF 2.0).")


if __name__ == "__main__":
    main()
