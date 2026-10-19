# Recognition fixtures

Captured from the visible boards at starbattlepuzzles.com on 2026-10-15 for interoperability testing. Each JSON file records the source URL and the visible gridcell accessibility labels and background colors. No hidden puzzle solution data was used.

The JPG files are screenshots returned by the browser capture tool. The `*-empty` boards are untouched; `*-partial` boards have a star at A1 and the website's automatically generated X marks. That test move is not necessarily correct for the puzzle. Solver tests use the empty board fixtures.

`10x10-page.jpg` is an additional full-page capture of the same empty 10×10 board.

`10x10-daily-page.png` is the user-provided October 6 daily puzzle screenshot, including the page header, navigation, timer, controls, and footer. Its JSON transcribes all region colors and marks (5 stars and 47 X's) as a serialized `Board` to verify recognition amid surrounding page content.

These files are test data, not a puzzle library shipped in the application.

`10x10-reported-position.json` records the player's board immediately before the surprising I9 hint reported on 2026-10-15. It is a serialized `Board`, used to check local I2/J2 reasoning and the later row I conflict.

`10x10-counting-position.json` transcribes the visible marks from the later screenshot that produced a three-deduction trial at D9. The teal preview X at D9 is not a placed mark. This fixture checks that the simpler count across rows H–J is explained first.
