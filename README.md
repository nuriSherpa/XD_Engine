# XD Engine

A lightweight 3D engine built in Rust with **winit** and **wgpu**. It features a custom glTF mesh loader, an interactive camera, and a Dear ImGui-style UI panel.

## Features

- **Custom glTF loader** — hand-rolled JSON + bin parser for static meshes
- **wgpu rendering** — cross-platform Vulkan/Metal/DX12 backend
- **Interactive camera** — orbit in 3D, pan in 2D grid mode
- **Mouse picking** — click to select objects or the scene camera
- **Gizmo axes** — visual XYZ axes on selected objects
- **Game & Scene views** — dual viewport rendering with egui integration

## Running

```bash
cargo run
```
