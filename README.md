# revise 
A tui anki client for my personal use. 

Revise is a command-line program used to schedule the review of items using
spaced repetition. Like other spaced-repetition software (Anki, Mnemosyne), the
scheduling algorithm is based on the FSRS algorithm. Unlike other
spaced-repetition software, this is not flashcard-based. An "item" in "revise"
is just a description of the thing you want to review. The actual information to
be reviewed is assumed to be elsewhere (in a text file somewhere, or in some
note-taking software, or written down in a notebook, or maybe carved into clay
tablets).

## Screenshot:
![](https://github.com/user-attachments/assets/422452d1-1b45-4f7b-a84f-db57903b9012)

## Keybindings
```
Tab             switch between reviews and decks
j|k             move up|down
a               add card
e               edit card
d               delete card  
r               review card
s               suspend card
q               quit 
```

# TODO
- [ ] also add ease in revlog (1234)
- [ ] fix fps
- [ ] revert fn. Also show what was chosen last review
