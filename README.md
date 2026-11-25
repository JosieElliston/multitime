

## ref

- [hsc2](https://github.com/HactarCE/Hyperspeedcube)
- [notation](https://hypercubing.xyz/drafts/hyper-puzzle-notation/)
- [twizzle](https://alpha.twizzle.net/edit)

## use

`cargo run [file]` (default `solve.txt`). Esc cycles between typing and twisting in either view; clicking the text box or a puzzle also switches. Hold either Shift for two-layer face turns. When twisting, keys work by whole tokens instead of characters: Left and Right step (with Shift, they select), and Backspace and Delete remove one, or its whole group if it's a bracket or separator. They stop at forks. Up and Down switch siblings, and Home and End go to the ends of the path. Right-click a sticker to turn its face clockwise, left-click for counterclockwise, and drag to orbit. The map is the text with one cell per token or `>`, and the caret goes between them. Click to move the caret, and double-click to also shorten the text box to its segment. Drag on the map to select a stretch of one path, or Shift+click to extend to there from the caret. The selection and clipboard work the same in every mode. Cmd+Z undoes and Cmd+Shift+Z redoes. Copying leaves out the `>` lines. Typing a `>` line forks, like Cmd+Enter.

| Keys (physical QWERTY positions) | Moves in the same order |
| --- | --- |
| `2 3 4 8 9 0` | `U2' M U' U M' M2'` |
| `Q W E R I O P` | `z' B L' y2' R B' z` |
| `A D F G H J K ;` | `y' L U' F' F U R' y` |
| `Z X C V B N M , . /` | `x' D L2' U2' D2 U2 R2' D' x x` |

`cargo test` also renders the UI to `target/ui-*.png`.
