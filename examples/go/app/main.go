//! Multi-package Go application for testing cross-package imports and resolution
//!
//! This example demonstrates:
//! - Cross-package imports using module paths
//! - Package visibility and exported/unexported identifiers
//! - Nested package structures
//! - Import aliases and dot imports
//! - Package initialization order

package main

import (
	"context"
	"fmt"
	"log"
	"time"

	// Import from local packages
	"app/config"
	"app/models"
	"app/services"
	"app/utils"

	// Import with aliases
	appConfig "app/config"
	userModel "app/models"
	authSvc "app/services"
)

func main() {
	fmt.Println("=== Multi-Package Go Application ===\n")

	// TEST 1: Cross-package type usage
	fmt.Println("1. Cross-package type instantiation:")
	user := models.NewUser("Alice", "alice@example.com", models.RoleAdmin)
	fmt.Printf("   Created user: %s\n", user.String())

	// TEST 2: Service initialization with cross-package dependencies
	fmt.Println("\n2. Service initialization:")
	settings := config.NewSettings()
	settings.LoadFromEnv()
	fmt.Printf("   Settings loaded: %+v\n", settings)

	db := services.NewDatabaseConnection(settings.DatabaseURL())
	if err := db.Connect(); err != nil {
		log.Fatalf("Database connection failed: %v", err)
	}
	fmt.Println("   Database connected ✓")

	authService := services.NewAuthService(db)
	fmt.Println("   Auth service initialized ✓")

	// TEST 3: Cross-package function calls
	fmt.Println("\n3. Cross-package function calls:")
	if utils.ValidateInput(user.Email()) {
		fmt.Println("   Email validation passed ✓")

		if err := authService.RegisterUser(user); err != nil {
			fmt.Printf("   Registration failed: %v\n", err)
		} else {
			output := utils.FormatOutput(fmt.Sprintf("User %s registered", user.Name()))
			fmt.Printf("   %s\n", output)
		}
	}

	// TEST 4: Generic/interface usage across packages
	fmt.Println("\n4. Cross-package interface usage:")
	processor := utils.NewDataProcessor(map[string]string{
		"transform": "uppercase",
	})
	processed := processor.Process("hello world")
	fmt.Printf("   Processed data: %s\n", processed)

	// TEST 5: Using aliased imports
	fmt.Println("\n5. Aliased import usage:")
	adminUser := userModel.NewUser("Admin", "admin@example.com", userModel.RoleAdmin)
	fmt.Printf("   Created admin via alias: %s\n", adminUser.String())

	authResult := authSvc.NewAuthService(db)
	fmt.Printf("   Auth service via alias: %T ✓\n", authResult)

	serverAddr := appConfig.NewSettings().ServerAddress()
	fmt.Printf("   Server address via alias: %s\n", serverAddr)

	// TEST 6: Error handling across packages
	fmt.Println("\n6. Cross-package error handling:")
	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	if token, err := authService.Authenticate(ctx, "alice@example.com", "wrongpassword"); err != nil {
		fmt.Printf("   Authentication failed: %v ✓\n", err)
	} else {
		fmt.Printf("   Authentication successful: %s\n", token)
	}

	// TEST 7: Package-level constants and variables
	fmt.Println("\n7. Package-level exports:")
	fmt.Printf("   Models version: %s\n", models.Version)
	fmt.Printf("   Utils module name: %s\n", utils.ModuleName)
	fmt.Printf("   Max name length: %d\n", models.MaxNameLength)

	db.Close()
	fmt.Println("\n=== All cross-package tests completed ===")
}

// Test functions (equivalent to integration tests)
func TestCrossPackageIntegration() error {
	user := models.NewUser("Test User", "test@example.com", models.RoleUser)

	settings := config.NewSettings()
	db := services.NewDatabaseConnection(settings.DatabaseURL())
	auth := services.NewAuthService(db)

	// Test that we can register a user across packages
	if err := auth.RegisterUser(user); err != nil {
		return fmt.Errorf("cross-package registration failed: %w", err)
	}

	return nil
}

func TestExportedTypes() error {
	// Test using exported types from different packages
	user := models.NewUser("Test", "test@example.com", models.RoleUser)
	if user.Role() != models.RoleUser {
		return fmt.Errorf("role mismatch")
	}

	// Test that error types work across packages
	_, err := services.ParseAuthError("user not found")
	if err == nil {
		return fmt.Errorf("expected error parsing to work")
	}

	return nil
}

func init() {
	// Package initialization - this runs before main()
	fmt.Println("[INIT] Main package initialized")
}

// Benchmark equivalent (for testing)
func BenchmarkCrossPackageCreation() {
	start := time.Now()
	for i := 0; i < 1000; i++ {
		_ = models.NewUser("Bench User", "bench@example.com", models.RoleUser)
	}
	duration := time.Since(start)
	fmt.Printf("Created 1000 users in %v\n", duration)
}
