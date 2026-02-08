module example.com/myproject

go 1.21

require (
    github.com/external/library v1.2.3
    golang.org/x/tools v0.13.0
)

replace github.com/old/module => ../legacy/module
replace example.com/internal => ./internal