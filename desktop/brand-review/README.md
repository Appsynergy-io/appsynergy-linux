# AppSynergy brand review — desktop / login art

**Active pool:** all of **v2** + all of **v3** (combined under `candidates-all/`).  
**Rule:** every candidate carries the official wordmark `[appsynergy]`.  
**Panel:** 1920×1200 (16:10); 3840×2400 is 2× master.

```bash
gwenview /home/imma/projects/appsynergy-linux/desktop/brand-review/candidates-all/*1920x1200*
# or
dolphin /home/imma/projects/appsynergy-linux/desktop/brand-review/candidates-all/
```

Reply with **desktop ID + login ID** (e.g. `v2-A` + `v3-D`). Lock = login unless you say otherwise.

---

## candidates-v2

| ID | File (1920×1200) | Role |
|----|------------------|------|
| **v2-A** | `v2-A-desktop-volumetric-1920x1200.png` | Desktop — volumetric mesh + god rays |
| **v2-B** | `v2-B-desktop-topology-1920x1200.png` | Desktop — orbital arcs / topology |
| **v2-C** | `v2-C-desktop-brand-depth-1920x1200.png` | Desktop — radial network brand depth |
| **v2-D** | `v2-D-login-atmospheric-1920x1200.png` | Login — horizon bloom + constellation |
| **v2-E** | `v2-E-login-brand-depth-1920x1200.png` | Login — radial + wordmark top |
| **v2-F** | `v2-F-desktop-procedural-sharp-1920x1200.png` | Desktop — procedural sharp (no AI) |

Also: matching `*-3840x2400.png` for each. Source: `candidates-v2/`

---

## candidates-v3 (lifted midtones)

| ID | File (1920×1200) | Role |
|----|------------------|------|
| **v3-A** | `v3-A-desktop-volumetric-1920x1200.png` | Desktop — volumetric, lifted |
| **v3-B** | `v3-B-desktop-topology-1920x1200.png` | Desktop — orbital topology, lifted |
| **v3-C** | `v3-C-desktop-radial-1920x1200.png` | Desktop — radial network, new gen |
| **v3-D** | `v3-D-login-horizon-1920x1200.png` | Login — horizon teal bloom |
| **v3-E** | `v3-E-lift-v2-volumetric-1920x1200.png` | Desktop — magick-lift of v2-A |
| **v3-F** | `v3-F-lift-v2-radial-1920x1200.png` | Desktop — magick-lift of v2-C |
| **v3-G** | `v3-G-lift-v2-login-1920x1200.png` | Login — magick-lift of v2-D |
| **v3-H** | `v3-H-desktop-procedural-lifted-1920x1200.png` | Desktop — procedural, lifted |

Also: matching `*-3840x2400.png` for each. Source: `candidates-v3/`

---

## Start menu (Kickoff) button

Hub-spoke mark from appsynergy-rs SVG (not AI). Panel-thick strokes for small sizes.

| File | Use |
|------|-----|
| `icons/start-menu/appsynergy-start.svg` | Transparent teal mark — default panel |
| `icons/start-menu/appsynergy-start-pad.svg` | Same on charcoal rounded pad |
| `icons/start-menu/appsynergy-start-{16..512}.png` | Raster sizes |
| `icons/start-menu/preview-on-dark-panel.png` | Side-by-side size preview |

```bash
gwenview /home/imma/projects/appsynergy-linux/desktop/brand-review/icons/start-menu/preview-on-dark-panel.png
```

Pick **start** (transparent) or **start-pad**. Wire into Plasma Kickoff after wallpaper pick.

---

## Layout

| Path | Contents |
|------|----------|
| `candidates-all/` | All v2 + v3 (28 files, prefixed `v2-` / `v3-`) |
| `candidates-v2/` | v2 only |
| `candidates-v3/` | v3 only |
| `icons/start-menu/` | Kickoff button SVG + PNGs |
| `refs/` | Wordmark + hub-spoke mark masters (SVG + PNG) |
