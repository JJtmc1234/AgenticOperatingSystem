# The square was never the character

JJ saw a lone square under Carl's name in the panel and asked what it was. It was fixed once,
by changing `\u{2588}` FULL BLOCK to `\u{258f}` LEFT ONE EIGHTH BLOCK, and it came straight
back, because the character was never the problem.

egui's default proportional font is Ubuntu-Light. **It contains no Block Elements at all.**
Neither glyph existed, so egui drew its missing glyph box both times, and a missing glyph box
is a square. Hack, the monospace face, has them, and the conversation is not drawn in Hack.

Verified with `fc-query` against the actual font files in
`~/.cargo/registry/src/*/epaint_default_fonts-*/fonts/`, rather than guessed.

## The rule

Before using a non ASCII glyph in a user interface, check the font actually contains it. A
missing glyph does not fail loudly, it draws a box, and a box looks enough like a deliberate
character that people fix the wrong thing.

In the panel, ASCII only. There is a test that fails on any non ASCII character the
conversation tries to draw, so this cannot come back a third time.

## The general shape

Two fixes that did not work means the diagnosis is wrong, not that the third variation will do
it. Stop and find the actual mechanism.
