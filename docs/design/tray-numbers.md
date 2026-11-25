# The percentage in the tray icon

Agreed with the user on 2026-11-25 ("Yes, start there"), as the first of
three: numbers in the icon (0.1.1), then a graphical drop-down, then a
floating display (`BACKLOG.md`).

## Why in the icon

Neither tray shows text beside an icon: Windows' notification area is
square (16 px at 100% scaling, up to 32 px), and ksni, Linux's tray, has
no label (Ayatana's `XAyatanaLabel` isn't among its properties). So the
number is drawn into the icon's own pixels, as the Mac app's "Icon +
Percentage" puts it beside its rings.

## What it shows

The outer ring's percentage (the 5-hour window, or the 7-day where the plan
has no 5-hour window), rounded, 0 to 100, in the middle of the rings, in
that ring's color (green, orange, red; or the 7-day's purples), which reads
on dark and light bars alike. Signed out, or before the first read, there's
no number: the rings are their faint tracks alone, as now.

## How it's drawn

- **The rings make room.** With a number, both rings are thin and at the
  icon's edge: each about 9% of the icon wide (1 px at 16 px, 3 at 32), a
  gap of about 4% between them (at least 1 px), so the middle is free.
  Without a number they're as before.
- **The digits** are a 3 × 5 pixel font of U4Ai's own (`icon::DIGITS`; the
  1 two pixels wide, so "100" is 10 pixels across), drawn opaque on whole
  pixels at the largest whole scale whose box fits inside the hole the
  rings leave (its corners within half a pixel of it): with two rings, 1
  at 16 and 22 px, 2 at 32, 4 at 64; with one, 1, 2, 3 and 6. A single
  digit takes the scale two would, so the number doesn't jump in size as
  it passes 9; "100" takes what fits, which can be smaller. Whole pixels
  keep them sharp where smoothing would blur a 5-pixel-tall digit into
  grey. The box is centred, rounded down to a whole pixel. The test
  `icons_are_written_out_where_asked`, with `U4AI_ICON_PICTURES=<dir>`,
  writes icons as raw RGBA to look at.
- At 100% the outer ring is full; "100" still fits, its corners grazing the
  inner ring at 16 px.

## Turning it off

A checkmark in the tray's menu, "Show percentage in the icon", on at first,
kept in `settings.json` (`percent_in_icon`). The drop-down's settings take
it over once there is one.

## Releasing

0.1.1 for Linux, the first update through the feed: an installed 0.1.0
finds it, installs it with its own `install.sh`, and starts it.
