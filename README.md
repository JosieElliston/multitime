# multitime

A 3x3x3 simulator for trying out fewest-moves tools for Hyperspeedcube 2, mainly a forking timeline.

![screenshot](ss.png)

## use

```sh
cargo run [file]  # default: solve.txt
```

### forks

A bubble is where the text can continue in several ways. It has a `>` for each branch. The active one is highlighted, and its text is shown after the bubble.

The caret can go into a bubble. Whichever `>` it's right after becomes active. Alt+Left and Alt+Right go into and out of a bubble without changing the active branch, as do Up and Down past one.

Type a `>` to split the text with a bubble, which puts the caret in it. In a bubble, `>` adds an empty branch, so `>>` is a new branch beside the text that was there. Anything else goes after the bubble. Pasting is the same as typing. In a comment, a `>` is just text. A comment ends at a bubble.

Or press Cmd+Enter to fork at the caret, or after the comment that it's in. If there's text after it, that becomes the other branch. Right before or after a bubble on the same line, it adds a branch to that one instead.

In a bubble, Backspace deletes a `>` along with its branch, though the only `>` just merges its text into what came before. Right after a bubble, Backspace deletes the bubble: the active branch's text stays, and the other branches are deleted. Deleting a selection does the same to each bubble in it. Copying a bubble copies just the active `>`.

### text mode and twist mode

Esc cycles between text mode, twist mode on the first puzzle, and twist mode on the second. Clicking the text box or a puzzle also switches.

| Key | Text mode | Twist mode |
| --- | --- | --- |
| Most keys | insert text | insert a twist (see below) |
| Left, Right | move by character | move by token |
| Up, Down | move by line | switch branches |
| Backspace, Delete | delete a character | delete a token (a bracket deletes its whole group) |
| Home, End | go to the ends of the line | go to the ends of the path |

In twist mode, a bubble counts as a token. Up and Down switch branches at the bubble that the caret is beside, or else the one before it. Hold Shift with Left, Right, Home, or End to select.

These work in both modes:

| Key | Action |
| --- | --- |
| Cmd+Z, Cmd+Shift+Z | undo, redo |
| Cmd+A, Cmd+C, Cmd+X, Cmd+V | select all, copy, cut, paste |
| Cmd+Enter | fork |
| Cmd+D | split the segment at the caret, without adding a branch |
| Cmd+J | delete the bubble before the caret, keeping the active branch |
| Cmd+[, Cmd+] | switch branches |
| Cmd+S | save |

### mouse

| Where | Action | Result |
| --- | --- | --- |
| Puzzle | right-click, left-click a sticker | turn its face clockwise, counterclockwise |
| Puzzle | drag anywhere in the view | rotate the view like HSC |
| Timeline | click | move the caret |
| Timeline | drag, or Shift+click | select along a path |

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

A saved file is plain text. Each branch starts with a `>` on its own line, and its contents are indented. The line breaks around the `>` aren't part of the text:

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
