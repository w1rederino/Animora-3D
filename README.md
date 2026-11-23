# Animora 3D

A desktop 3D animation app in the spirit of Maya, written in Rust with
[egui/eframe](https://github.com/emilk/egui). English and German UI.
Made with [Craft Anything](https://getartcraft.com/apps/craft-anything) and Claude Code.

## Features

- **Modeling** – primitives (cube, sphere, cylinder, cone, plane, torus),
  vertex/face editing, extrude, smooth, groups, OBJ import/export.
- **Animation** – keyframes on every channel, channel box, timeline with
  auto-key, graph editor (spline/linear/flat/stepped tangents), dope sheet,
  and a clip list to switch between several imported animations.
- **Rigging** – joints, IK handles, smooth skin binding (up to 8 weights).
- **Rendering** – software renderer with lights, cameras and textures;
  PNG image and image-sequence output.
- **Call of Duty tools** (like CoDMayaTools) – import/export
  `.xmodel_export` and `.xanim_export`; one animation drives the joints of
  several models (arms, weapon, scope); *Attach Weapon to Rig*.
- **Cast** (`.cast`) – import/export of models and animations
  (absolute/relative/additive), same-named joints follow the animation.
- Script editor / command line, save and load scenes (`.anim3d`),
  drag and drop files onto the window or the program icon.

## Build and run

Needs [Rust](https://rustup.rs) (stable) on Windows with the Visual Studio
C++ build tools ("Desktop development with C++") and the Windows SDK.

```
cargo run --release
```

Open files at start: `animora file.xmodel_export file.xanim_export`.
Save a screenshot of the window and exit: `animora --screenshot out.png`.

## Files in this repository

Needed to build:

| File / folder         | What it is                                           |
|-----------------------|------------------------------------------------------|
| `src/`                | all program code                                     |
| `Cargo.toml`          | project name and the libraries it uses               |
| `Cargo.lock`          | exact library versions, so every build is the same   |
| `rust-toolchain.toml` | which Rust version to use                            |

Also included: `README.md`, `animora.ico` (program icon).
