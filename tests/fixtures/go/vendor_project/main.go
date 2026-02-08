package main

import (
    "fmt"
    "github.com/external/library" // This will be resolved from vendor/
    "github.com/gin-gonic/gin"    // This would also be in vendor/
)

func main() {
    fmt.Println("Vendor project example")
    
    // Use vendored library
    client := library.NewClient("http://api.example.com")
    client.Connect()
    data, _ := client.GetData()
    fmt.Printf("Data: %s\n", data)
    
    // Use vendored gin
    r := gin.Default()
    r.GET("/ping", func(c *gin.Context) {
        c.JSON(200, gin.H{"message": "pong"})
    })
}