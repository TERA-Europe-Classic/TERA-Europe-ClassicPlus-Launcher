# Launcher Design Specification

This document outlines the desired layout and functional requirements for the TERA Europe launcher. It summarises the main elements that should be present in the user interface and key behaviour expected from the application.

## Layout Overview

- **Top menu bar** with the following items:
  - `Start`
  - `Discord`
  - `Settings` (opens a menu)
  - `Support`
- **Language selector** in the top right corner offering German (DE), English (EN), French (FR) and Russian (RU). Switching language should not require restarting the launcher.
- **Privacy link** placed in the upper right corner that directs users to: <https://forum.crazy-esports.com/index.php?datenschutzerklaerung/>.
- A **large central banner** (for example displaying the TERA Europe logo).
- **Three info tiles** positioned below the banner as well as **one tile on the right** side of the banner.
- A **footer** at the bottom of the window.

## Download and Patch Status

The launcher needs to display detailed progress information while downloading game updates:

- Current transfer speed (MB/s).
- Remaining data and time left.
- Percentage completed and a progress bar.
- Number of patches still required.
- A pause/resume control in the bottom right corner.

## Localisation

All texts, buttons and other contents must be available in German, English, French and Russian. The user can switch languages on the fly and the interface updates without a restart.

## Functional Behaviour

- Implement a patch/download system that shows progress in the manner described above.
- Provide clear status indicators for the main action button: `Play`, `Pause` or `Update running` depending on the current state.

These points describe the overall goals for the launcher interface and behaviour and serve as guidance when implementing new features or adjusting the UI.
