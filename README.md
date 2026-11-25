

## ref

- [hsc2](https://github.com/HactarCE/Hyperspeedcube)
- [notation](https://hypercubing.xyz/drafts/hyper-puzzle-notation/)
- [twizzle](https://alpha.twizzle.net/edit)

## use

`cargo run [file]` (default `solve.txt`). Esc leaves the text box. Hold either Shift for two-layer face turns. Outside the text box, Backspace undoes, Shift+Backspace redoes, and the arrow keys scrub. Right-click a sticker to turn its face clockwise, left-click for counterclockwise, and drag to orbit. Drag on the map to scrub, and double-click a dot to shorten the text box to its segment.

| Keys (physical QWERTY positions) | Moves in the same order |
| --- | --- |
| `2 3 4 8 9 0` | `U2' M U' U M' M2'` |
| `Q W E R I O P` | `z' B L' y2' R B' z` |
| `A D F G H J K ;` | `y' L U' F' F U R' y` |
| `Z X C V B N M , . /` | `x' D L2' U2' D2 U2 R2' D' x x` |

`cargo test` also renders the UI to `target/ui-*.png`.
