# Link and destination rectangles under `\pdfsetmatrix`
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_ship.rs`

## Context
lscape and pdflscape rotate pages with `\pdfsave`/`\pdfsetmatrix`/`\pdfrestore`. pdfTeX
transforms annotation rectangles and destinations through the current matrix (utils.c), so a
link on rotated text gets the rotated rectangle.

## Decision
The walker keeps utils.c's `matrix_stack`: each `\pdfsetmatrix` about the current point,
composed with the matrix before it. It also keeps the stack depth at each `\pdfsave`, and
`\pdfrestore` truncates to that depth. This happens on pages only (`page_mode`).
`set_rect_dimens` goes through `matrixtransformrect` (the box around the four corners,
rounded half away from zero) before the margin is added. A running link's right edge uses
`matrixrecalculate` on the last rectangle transformed. Under a matrix, every destination
view takes its place from the transformed rectangle.

## Rejected
- Matching lscape's `Doc-Start`. pdfTeX's `\pdfdest ... xyz` never initialises the node's
  width, height and depth, yet `do_dest` reads them under a matrix. The result is leftover
  memory (pdfTeX writes y = 4201bp), so lscape stays out of the corpus.

## Measured
- lscape: links 0/1 to 1/1. pdflscape stays at zero shift.
