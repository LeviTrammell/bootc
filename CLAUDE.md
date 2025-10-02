# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a bootc (bootable container) project for creating Fedora-based bootable container images, specifically targeting Raspberry Pi hardware. The project uses container-based workflows to build immutable, ostree-based operating system images.

## Architecture

The project follows a modular architecture:

- **Container Images**: Built from Fedora bootc base images with custom configurations
- **Variants**: Different hardware/configuration targets (currently `pi` for Raspberry Pi)
- **Build System**: Uses GitHub Actions for CI/CD with container-based build workflows
- **Image Generation**: Uses bootc-image-builder to create bootable disk images from containers

Key components:
- `variants/pi/`: Raspberry Pi specific configuration
  - `Containerfile`: Multi-stage container build definition
  - `config.toml`: bootc-image-builder configuration with user setup
  - USB gadget networking setup for headless access
  - First-boot partition resize functionality
  - Custom bootloader shim for firmware management

## Common Development Commands

### Building Container Images
```bash
# Build the Raspberry Pi container image
task containers:pi

# Or directly with podman (for local development)
podman build \
  --platform linux/arm64 \
  --build-arg FEDORA_MAJOR_VERSION=42 \
  -f ./variants/pi/Containerfile \
  -t ghcr.io/levitrammell/pi \
  .
```

### Building Bootable Disk Images
```bash
# Build a bootable disk image for Raspberry Pi
task images:pi

# The output will be in dist/pi/ directory
```

### Task Management
```bash
# List all available tasks
task --list-all

# Default task (shows all tasks)
task
```

## Key Files and Locations

- `Taskfile.yml`: Main task runner configuration
- `.taskfiles/`: Task definitions for containers and images
- `.github/workflows/`: CI/CD pipeline definitions
- `variants/pi/`: Raspberry Pi specific files and scripts
- `scripts/`: Common installation scripts

## Build Variables

- `FEDORA_MAJOR_VERSION`: Fedora version to use (default: 42)
- `ARCH`: Target architecture (arm64 for Pi)
- `BOOTC_IMAGE_TYPE`: Disk image format (default: raw)
- `BASE_REPO`: Container registry (ghcr.io/levitrammell)

## CI/CD Pipeline

The project uses GitHub Actions with two main workflows:
1. **build.yml**: Scheduled daily builds and push triggers
2. **build-image.yml**: Reusable workflow for building and pushing container images

Builds run on appropriate runners based on architecture (standard for amd64, ARM runners for arm64).