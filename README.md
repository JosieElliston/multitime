# multitime

a 3x3x3 simulator for trying out FMC tools for [Hyperspeedcube 2](https://github.com/hactarCE/Hyperspeedcube/), mainly a forking timeline.

![screenshot](ss.png)

```sh
cargo run [file]  # default: solve.txt
```


## elements

there are four elements:

- two **puzzle views** show the state at the caret.
- the **timeline** shows the full tree: a dot for each token, and a tick for each line break that doesn't end a comment.
- the **text box** shows the notation of one path down the tree.

there's one caret and one selection, shared by the text box and the timeline.

## modes

there are two input modes: **text mode** edits by character, while **twist mode** edits by token. a token is a twist, a bracket, a separator, a multiplier, a comment, or a bubble. a multiplier is a token of its own, so between `]` and `2` is the state after one pass.

esc cycles between focus on the text box and the two puzzles.

| Key | Text mode | Twist mode |
| --- | --- | --- |
| Most keys | insert text | insert a twist (see [twist keys](#twist-keys)) |
| Left, Right | move by character | move by token |
| Option+Left, Option+Right | move by token | move by run (see [mouse](#mouse)) |
| Shift+Option+Left, Shift+Option+Right | select by meaning (see [selecting by meaning](#selecting-by-meaning)) | the same, but a run counts as one |
| Cmd+Left, Cmd+Right | go to the ends of the line | go to the ends of the segment |
| Up, Down | move by line | switch branches at the bubble before the caret |
| Backspace, Delete | delete a character | delete a token (a bracket deletes its whole group) |
| Option+Backspace, Option+Delete | delete a token | delete a run |
| Cmd+Backspace, Cmd+Delete | delete to the ends of the line | delete to the ends of the segment |
| Home, End | go to the ends of the line | go to the ends of the path |

## forks

a **bubble** has a `>` for each child, with one active child. it's on a line of its own, and the line breaks around it aren't part of the text: they only separate it from the lines before and after it. so an empty segment has no line, and two bubbles can be on adjacent lines.

the caret can go into a bubble, before its first `>` or after one of them, and whichever `>` it's right after becomes active. Up and Down onto a bubble's line go after the active `>`. to pass a bubble without changing the active branch, hold option. in text mode, Shift+Left at a bubble's line's start or Shift+Right at its end selects all of its `>`s, like Shift+Cmd+Left or Right.

in twist mode and on the timeline, a branch starts right after its `>`, rather than at the start of its first line.

cmd+[ and cmd+], or clicking a `>`, also switch branches.

## mouse

in both the text box and the timeline:

- double-click selects a token. on a bracket, it selects the whole group, without its multiplier, and on the multiplier, with it. on a separator, it selects what's inside the group, since the separator is. in a comment, it selects a word.
- triple-click selects a run: the tokens up to the nearest bracket, separator, or line break, such as one side of a commutator, or a line and its comment. on a bracket, it selects what's inside the group, and on a separator, the whole group.
- dragging after either extends by the same unit. a drag over a bracket takes in its whole group, and its multiplier only if the drag reaches it.

a selection that starts in a bubble stays in it, and selects `>`s. any other selection takes in all of a bubble or none of it.

### selecting by meaning

notation is a tree. its leaves are moves, pauses, comments and multipliers. a group's children are its sides (one, or two around a `,` or `:`), and each side is a list of leaves and groups. brackets and separators are how a group is written, not leaves, and line breaks and bubbles don't count, so a selection never ends beside one. a multiplier is a leaf right after its group. a semantic selection is a run of siblings.

Shift+Option+an arrow:

- snaps a selection that isn't semantic to the smallest one that covers it, such as a caret partway through a move to that move.
- toward the caret's end of the selection, or either way from a caret or a double or triple click, grows it: adds the next sibling that way. with none, it takes all of the siblings, and with all of them, it selects the parent. so from `[U|' R : F]2' U' (R' L)2`, Right selects `U'`, `U' R`, `U' R : F`, `[U' R : F]`, `[U' R : F]2'`, and so on, a sibling at a time.
- toward the anchor's end, undoes a grow: removes the sibling at the caret's end, or goes into a lone group. that's exact, but for a grow that took all of the siblings from partway through them: undoing that removes one from the caret's end.

it goes across bubbles. right after a `>` is the branch's start, as on the timeline. before a bubble's first `>`, or with `>`s selected, it's Shift+Cmd+arrow instead, so it stays in the bubble, as with the mouse. in twist mode, a run of moves on one line counts as one sibling, so from the middle of one, it goes to its end.

### context menu

right-click in the text box or on the timeline. in the selection, it has cut, copy and paste, and these FMC tools for a selection of text:

- **group** wraps the selection in `( )`.
- **commutator** and **conjugate** wrap the selection in `[ ]`, with the `,` or `:` at the gap nearest the click, but not inside a group that the selection doesn't have all of.
- **NISS** wraps it in `^( )`.
- **expand** writes out each commutator and conjugate whose brackets are in the selection: `[A, B]` as `A B A' B'` and `[A: B]` as `A B A'`. a multiplier in the selection is written out too, so `[A: B]2` is `A B A' A B A'`, and one that isn't stays, as `(A B A')2`.
- **invert** replaces the selection with moves that undo it: lines in reverse order, each keeping its comment, and their moves reversed with their primes toggled. it needs a selection in one segment that cuts no group.

the first four work across forks, since they're just brackets. elsewhere, a right click moves the caret there, and the menu only pastes.

### making forks

- outside a comment or bubble, type `>` to split the timeline. the fork takes the place of the whitespace around it.
- inside a bubble, `>` adds an empty branch after the `>` before the caret, or before the first `>`, a first one.
- before a bubble's first `>`, anything else goes on a line of its own before the bubble.

### deleting forks

- in a bubble, Backspace deletes a `>` and its branch, unless it's the only `>`, in which case its text is kept. in twist mode, that's Backspace at a branch's start.
- Backspace at the start of the line after a bubble, or Delete at the end of the line before it, deletes the bubble like Cmd+J.

### copy/paste

- selected `>`s copy their branches whole, written as in a [file](#files). cutting all of a bubble's `>`s deletes all of its branches.
- a selection across a bubble copies only its active branch, with the `>` on a line of its own.
- pasted text that starts with a `>` is branches. they go in a bubble at the caret, where Cmd+Enter would put an empty one, and mid-text, the text after the caret is the first branch.
- otherwise, pasted text goes in at the caret, and the text after the caret continues from its last fork.

### moving lines

Option+Up and Option+Down move the lines that the selection or caret is on, past the line above or below, which can be a bubble's. past a bubble, they go into or out of its active branch only, as the text box reads. a selection across bubbles moves their lines with it, but not past another bubble's line. before a bubble's first `>`, they move the bubble's line.

on a bubble's line, they move the selected `>`s, or the one before the caret, among their siblings. in twist mode, that's at a branch's start.

Shift+Option+Up and Shift+Option+Down duplicate the lines instead, within one segment, or the branches, leaving the caret on the upper or lower copy.

## keys

These work in both modes:

| Key | Action |
| --- | --- |
| Cmd+Z, Cmd+Shift+Z | undo, redo |
| Cmd+A, Cmd+C, Cmd+X, Cmd+V | select all, copy, cut, paste |
| Cmd+Enter | fork at the gap between tokens at or after the caret (so after a comment, not in it), in place of its spaces. right before a bubble, add a first branch to it instead; right after one, or inside one, a branch after the caret's; before a bubble's first `>`, a first one. |
| Cmd+J | delete the bubble before the caret's segment, keeping the active branch. its line goes, and a line break stays between the text before and after it. |
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
