# multitime

A 3x3x3 simulator for trying out fewest-moves tools for Hyperspeedcube 2, mainly a timeline that forks.

The notation is the ground truth: the puzzle shows whatever the text before the caret does.

## use

```sh
cargo run [file]  # default: solve.txt
```

### forks

A `>` is like a line break that can have several next lines. The text box shows all of a fork's branches as a row of `>`s, with the current one highlighted. Click another to switch to it.

Type `>` to fork at the caret. If there's text after it, that becomes the other branch. Delete a `>` to merge its branch into what came before; any other branches then continue from the end of the merged text.

### typing and twisting

Esc cycles between typing, twisting on the first puzzle, and twisting on the second. Clicking the text box or a puzzle also switches.

| Key | Typing | Twisting |
| --- | --- | --- |
| Most keys | insert text | insert a twist (see below) |
| Left, Right | move by character | move by token |
| Up, Down | move by line | switch branches |
| Backspace, Delete | delete a character | delete a token (a bracket deletes its whole group) |
| Home, End | go to the ends of the line | go to the ends of the path |

In twisting mode, a `>` counts as a token. Hold Shift with Left, Right, Home, or End to select.

These work in both modes:

| Key | Action |
| --- | --- |
| Cmd+Z, Cmd+Shift+Z | undo, redo |
| Cmd+A, Cmd+C, Cmd+X, Cmd+V | select all, copy, cut, paste (copying and cutting leave out `>`s) |
| Cmd+Enter | fork |
| Cmd+D | split the segment at the caret, without adding a branch |
| Cmd+J | merge this segment into its parent |
| Cmd+[, Cmd+] | switch branches |
| Cmd+S | save |

### mouse

| Where | Action | Result |
| --- | --- | --- |
| Puzzle | right-click, left-click a sticker | turn its face clockwise, counterclockwise |
| Puzzle | drag | rotate the view |
| Map | click | move the caret |
| Map | double-click | move the caret, and end the text box's path at that segment |
| Map | drag, or Shift+click | select along a path |

### twist keys

| Keys | Moves, in the same order |
| --- | --- |
| `2 3 4 8 9 0` | `U2' M U' U M' M2'` |
| `Q W E R I O P` | `z' B L' y2' R B' z` |
| `A D F G H J K ;` | `y' L U' F' F U R' y` |
| `Z X C V B N M , . /` | `x' D L2' U2' D2 U2 R2' D' x x` |

### notation

| Notation | Meaning |
| --- | --- |
| `R U2 F'`, `Rw`, `r`, `2-3R` | face, wide, and layer turns |
| `M E S`, `x y z`, `@R` | slices and rotations |
| `(R U)3` | repeat |
| `[R, U]`, `[R: U]` | commutator, conjugate |
| `^(R U)` | moves on the inverse (NISS) |
| `.` | pause |
| `// text` | comment |

### files

A saved file is plain text. Each branch starts with a `>`, and its contents are indented:

```text
R' U' F D2 L
>
  // cross
  R F D2 B
>
  // cross, on the inverse
  ^(U F')
```

## dev

`cargo test` also renders the UI to `target/ui-*.png`.

## ref

- [hsc2](https://github.com/HactarCE/Hyperspeedcube)
- [notation](https://hypercubing.xyz/drafts/hyper-puzzle-notation/)
- [twizzle](https://alpha.twizzle.net/edit)
