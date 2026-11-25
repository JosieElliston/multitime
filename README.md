

## ref

- [hsc2](https://github.com/HactarCE/Hyperspeedcube)
- [notation](https://hypercubing.xyz/drafts/hyper-puzzle-notation/)
- [twizzle](https://alpha.twizzle.net/edit)

## use

`cargo run [file]` (default `solve.txt`). Esc cycles between typing and twisting in either view; clicking the text box or a puzzle also switches. Hold either Shift for two-layer face turns. When twisting, the arrow keys scrub. Right-click a sticker to turn its face clockwise, left-click for counterclockwise, and drag to orbit. Click on the map to move the caret, and double-click a dot to shorten the text box to its segment. Drag on the map to select a stretch of the path, or Shift+click to extend to there from the caret. Clipboard and selection keys work in every mode, but Backspace and Delete only when typing. Cmd+Z undoes and Cmd+Shift+Z redoes. Copying leaves out the `>` lines.

| Keys (physical QWERTY positions) | Moves in the same order |
| --- | --- |
| `2 3 4 8 9 0` | `U2' M U' U M' M2'` |
| `Q W E R I O P` | `z' B L' y2' R B' z` |
| `A D F G H J K ;` | `y' L U' F' F U R' y` |
| `Z X C V B N M , . /` | `x' D L2' U2' D2 U2 R2' D' x x` |

`cargo test` also renders the UI to `target/ui-*.png`.
