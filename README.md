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
| Option+Backspace, Option+Delete | delete a token | delete a run |
| Cmd+Backspace, Cmd+Delete | delete to the ends of the line | delete to the ends of the segment |
| Home, End | go to the ends of the line | go to the ends of the path |

## forks

a **bubble** has a `>` for each child, with one active child. it's on a line of its own, and the line breaks around it aren't part of the text.

the caret can go into a bubble, before its first `>` or after one of them, and whichever `>` it's right after becomes active. Up and Down onto a bubble's line go after the active `>`. to pass a bubble without changing the active branch, hold option.

cmd+[ and cmd+], or clicking a `>`, also switch branches.

## mouse

in both the text box and the timeline:

- double-click selects a token. on a bracket or separator, it selects the whole group. in a comment, it selects a word.
- triple-click selects a run: the tokens up to the nearest bracket, separator, or line break, such as one side of a commutator, or a line and its comment. on a bracket or separator, it selects what's inside the group.
- dragging after either extends by the same unit. a drag over a bracket takes in its whole group.

a selection that starts in a bubble stays in it, and selects `>`s. any other selection takes in all of a bubble or none of it.

### making forks

- outside a comment or bubble, type `>` to split the timeline. the fork takes the place of the whitespace around it.
- inside a bubble, `>` adds an empty branch.
- before a bubble's first `>`, anything else goes on a line of its own before the bubble.

### deleting forks

- in a bubble, Backspace deletes a `>` and its branch, unless it's the only `>`, in which case its text is kept.
- Backspace at the start of the line after a bubble, or Delete at the end of the line before it, deletes the bubble like Cmd+J.

### copy/paste

- selected `>`s copy their branches whole, written as in a [file](#files). cutting all of a bubble's `>`s deletes all of its branches.
- a selection across a bubble copies only its active branch, with the `>` on a line of its own.
- pasted text that starts with a `>` is branches. they go in a bubble at the caret, where Cmd+Enter would put an empty one, and mid-text, the text after the caret is the first branch.
- otherwise, pasted text goes in at the caret, and the text after the caret continues from its last fork.

### moving lines

Option+Up and Option+Down move the lines that the selection or caret is on, past the line above or below, which can be a bubble's. past a bubble, they go into or out of its active branch only, as the text box reads. on a bubble's line, they move the selected `>`s, or the one before the caret, among their siblings, and before the first `>`, they move the bubble's line.

Shift+Option+Up and Shift+Option+Down duplicate the lines instead, or on a bubble's line, the `>`s and their branches, leaving the caret on the upper or lower copy.

## keys

These work in both modes:

| Key | Action |
| --- | --- |
| Cmd+Z, Cmd+Shift+Z | undo, redo |
| Cmd+A, Cmd+C, Cmd+X, Cmd+V | select all, copy, cut, paste |
| Cmd+Enter | fork at the gap between tokens at or after the caret (so after a comment, not in it), in place of its spaces. right before a bubble, add a first branch to it instead; right after, a last one; inside one, an empty branch. |
| Cmd+J | delete the bubble before the caret's segment, keeping the active branch. its line goes, and the line break before it stays. |
| Option+Up, Option+Down | move lines or branches (see [moving lines](#moving-lines)) |
| Shift+Option+Up, Shift+Option+Down | duplicate lines or branches |
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
