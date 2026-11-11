# Capture sheet: DF-NNN <title>

One sheet per scenario run and side. Copy this file to `captures/DF-NNN/sheet.md`. Verbatim means exactly as displayed, including punctuation and capitalization.

## Run

- Scenario file:
- Side (ours / original):
- Date and tester:
- IDE name, build and OS:
- Plugin and exact version (ours: git commit; original: `com.haulmont.jpab` 263.7307.112, archive SHA-256 checked yes/no):
- Fixture and fixture commit:
- Input file and caret/selection position:
- Entry point used (menu path, shortcut, intention, gutter, tool window):
- Input variants run (names of the `captures/DF-NNN/<side>/<variant>/` directories):

## Dialogs and prompts

For every dialog or popup, in order:

| Step | Screenshot name | Title (verbatim) | Fields shown | Defaults preselected | Fields disabled or hidden and why |
| --- | --- | --- | --- | --- | --- |

- Option lists offered (verbatim, in displayed order):
- Validation messages shown for bad input (input given, message verbatim):
- Was a preview shown before applying? yes/no. Screenshot name:

## Refusals and messages

| Situation | Message or notification (verbatim) | Where shown (dialog, balloon, status bar, editor) |
| --- | --- | --- |

## Result

- Files created (relative path):
- Files edited (relative path):
- Files NOT touched that you expected to change:
- Imports added or reordered by the IDE:
- Reformat applied after generation? yes/no:
- Caret position and selection after the action:
- Files opened automatically:
- Diagnostics list captured (`diagnostics.json`)? yes/no. Quick fixes offered per diagnostic:

## Undo

- Undo (one step) restored every file to the original content? yes/no/partial
- Number of undo steps needed:
- Files left changed after undo:
- Redo gave the same result? yes/no

## Differences noticed that outputs do not show

Free text: timing, progress indicators, extra confirmations, ordering of offered items, things the original did that ours does not and the reverse.

## Comparator result

- Command line used:
- Normalizers (and any extra flags):
- Result per variant: identical-after-normalization / cosmetic / semantic / files only on one side.
- Reviewer note on each cosmetic difference (confirm it is cosmetic):
