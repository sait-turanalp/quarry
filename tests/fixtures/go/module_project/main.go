package main

import (
    "fmt"
    "example.com/myproject/internal/config"
    "example.com/myproject/pkg/utils"
    "./local" // relative import
    "../shared" // relative import
)

func main() {
    fmt.Println("Module project example")
    
    cfg := config.New()
    result := utils.Process("data")
    
    local.DoSomething()
    shared.Helper()
}