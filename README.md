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

## Screenshots

<img width="2560" height="1389" alt="{A8E04F12-D866-4099-B5CD-5BA8F4F43005}" src="https://github.com/user-attachments/assets/20b145fb-4482-4aba-a1aa-196c37932271" />

<img width="2560" height="1389" alt="{F4F20ECD-6F6F-4139-B3C7-0E3D7E221A1C}" src="https://github.com/user-attachments/assets/60c7e0c6-9eaa-4e47-bc43-ad4eea44ae70" />

<img width="820" height="461" alt="gh gif" src="https://github.com/user-attachments/assets/90b0d7a0-ae52-4e77-a9ea-89917b164110" />

