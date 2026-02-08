// Test file for cross-module call tracking in Go
package main

import (
	"fmt"
	"github.com/example/project/config"
	"github.com/example/project/utils/helper"
)

// InitConfigFile initializes the configuration file
func InitConfigFile() {
	// This should be tracked as calling config.InitGlobalDirs
	config.InitGlobalDirs()

	// This should be tracked as calling utils/helper.ProcessData
	helper.ProcessData()
}

// InitGlobalDirs initializes global directories (local function)
func InitGlobalDirs() {
	fmt.Println("Initializing global directories locally")
}

// ProcessLocalData processes data locally
func ProcessLocalData() {
	// This calls the local InitGlobalDirs
	InitGlobalDirs()

	// This calls the imported config.InitGlobalDirs
	config.InitGlobalDirs()
}