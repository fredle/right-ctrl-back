# Right Ctrl Back

Turn the AI key on new laptops back into Right Ctrl, or anything else you choose.

## Problem

Most laptops sold since 2024 replaced Right Ctrl with a Copilot key that sends the chord Win+Shift+F23. Owners want Ctrl back. PowerToys remaps are reported as flaky when the key is held as a modifier and have been reset by Windows updates; the rest is AutoHotkey scripts.

## Features (1.0)

- Low-level keyboard hook that recognises the Win+Shift+F23 chord and suppresses it
- Maps it to a true held Right Ctrl that works as a modifier with other keys, including Shift and Alt combinations
- Alternative targets: menu key, a media key, any single key, or launching a chosen app
- Key tester in the egui window showing what the key sends and what the app emits
- Tray toggle to pause, and a clear note that it cannot act inside elevated windows

## Non-goals

- No general keyboard remapper for other keys
- No registry Scancode Map changes or elevation
- No macros or text expansion
- No use of the Copilot name in the app or Store title

## Evidence of demand

- https://community.acer.com/en/discussion/728535/need-my-right-ctrl-key-back-not-copilot-powertoys-remap-doesnt-work
- https://www.elevenforum.com/t/remap-co-pilot-key-back-to-a-ctrl-key-using-power-toys.24830/latest
- https://learn.microsoft.com/en-us/answers/questions/5671722/remap-the-copilot-key-to-ctrl-in-windows-11
- https://h30434.www3.hp.com/t5/AI-PCs/HP-please-bring-back-the-right-Ctrl-key-Proposed-solution/m-p/9343623
