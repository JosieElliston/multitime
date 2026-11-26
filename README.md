# multitime

a 3x3x3 simulator for trying out FMC tools for [Hyperspeedcube 2](https://github.com/hactarCE/Hyperspeedcube/), mainly a forking timeline.

![screenshot](ss.png)

```sh
cargo run [file]  # default: solve.txt
```


## elements

there are four elements:

- two **puzzle views** show the state at the caret.
- the **timeline** shows the full tree.
- the **text box** shows the notation of one path down the tree.

there's one caret and one selection, shared by the text box and the timeline.

## modes

there are two input modes: **text mode** edits by character, while **twist mode** edits by token. a token is a twist, a bracket, a comment, or a bubble.

esc cycles between focus on the text box and the two puzzles.

| Key | Text mode | Twist mode |
| --- | --- | --- |
| Most keys | insert text | insert a twist (see [twist keys](#twist-keys)) |
| Left, Right | move by character | move by token |
| Option+Left, Option+Right | move by token | move by run (see [mouse](#mouse)) |
| Cmd+Left, Cmd+Right | go to the ends of the line | go to the ends of the segment |
| Up, Down | move by line | switch branches at the bubble before the caret |
| Backspace, Delete | delete a character | delete a token (a bracket deletes its whole group) |
| Home, End | go to the ends of the line | go to the ends of the path |

## forks

a **bubble** has a `>` for each child, with one active child.

the caret can go into a bubble, and whichever `>` it's right after becomes active. to pass a bubble without changing the active branch, hold option.

cmd+[ and cmd+], or clicking a `>`, also switch branches.

## mouse

in both the text box and the timeline:

- double-click selects a token. on a bracket or separator, it selects the whole group. in a comment, it selects a word.
- triple-click selects a run: the tokens up to the nearest bracket, separator, or line break, such as one side of a commutator, or a line and its comment. on a bracket or separator, it selects what's inside the group.
- dragging after either extends by the same unit. a drag over a bracket takes in its whole group.

### making forks

- outside a comment or bubble, type `>` to split the timeline.
- inside a bubble, `>` adds an empty branch.

### deleting forks

- in a bubble, Backspace deletes a `>` and its branch. unless it's the only `>` in which case its text is kept.
- right after a bubble, Backspace deletes the bubble's entire subtree.

### copy/paste

copying a bubble copies just its active `>`.

## keys

These work in both modes:

| Key | Action |
| --- | --- |
| Cmd+Z, Cmd+Shift+Z | undo, redo |
| Cmd+A, Cmd+C, Cmd+X, Cmd+V | select all, copy, cut, paste |
| Cmd+Enter | right before a bubble, add a first branch to it; right after, a last one; otherwise type a `>`, even in a comment or in twist mode. |
| Cmd+J | delete the bubble before the caret's segment, keeping the active branch |
| Cmd+[, Cmd+] | switch branches |
| Cmd+S | save |

### twist keys

| Keys | Moves, in the same order |
| --- | --- |
| `2 3 4 8 9 0` | `U2' M U' U M' M2'` |
| `Q W E R I O P` | `z' B L' y2' R B' z` |
| `A D F G H J K ;` | `y' L U' F' F U R' y` |
| `Z X C V B N M , . /` | `x' D L2' U2' D2 U2 R2' D' x x` |

Hold Shift for wide moves.

## notation

| Notation | Meaning |
| --- | --- |
| `R U2 F'`, `Rw`, `r`, `2-3R` | face, wide, and layer turns |
| `M E S`, `x y z`, `@R` | slices and rotations |
| `(R U)3` | repeat |
| `[R, U]`, `[R: U]` | commutator, conjugate |
| `^(R U)` | moves on the inverse (NISS) |
| `.` | pause |
| `// text` | comment |

## files

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
`cargo test --test ui -- --ignored` renders `ss.png`.

## ref

- [notation](https://hypercubing.xyz/drafts/hyper-puzzle-notation/)
- [twizzle](https://alpha.twizzle.net/edit)
