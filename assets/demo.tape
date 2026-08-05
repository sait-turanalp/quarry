# Terminal demo, recorded by vhs so it can be regenerated when the output changes.
#
#   brew install vhs
#   QUARRY_DEMO_REPO=/path/to/django vhs assets/demo.tape
#
# One command, one question, asked the way a person would ask it. The point is not that
# the answer is impressive, it is that nothing about the question looks like a search
# query: "too common" is the only phrase shared with the code, and the file it lands on
# is the validator that implements the rule.

Output assets/images/demo.gif

Require quarry

Set Shell "bash"
Set FontSize 30
Set Width 2360
Set Height 820
Set Padding 48
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Set BorderRadius 10
Set TypingSpeed 55ms
Set Framerate 24

# Set the stage off-camera: a clean prompt inside the repository being searched.
Hide
Type "cd ${QUARRY_DEMO_REPO:-.} && PS1='$ ' && clear"
Enter
Sleep 1s
Show

Sleep 800ms
Type `quarry search "where do we decide a password is too common" --limit 3`
Sleep 600ms
Enter
Sleep 5s
