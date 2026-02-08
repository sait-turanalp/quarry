// Package imports demonstrates various Go import patterns
package imports

// Standard library imports
import (
    "fmt"
    "os"
    "path/filepath"
    "strings"
)

// External module imports
import (
    "github.com/gin-gonic/gin"
    "github.com/lib/pq"
    "golang.org/x/crypto/bcrypt"
)

// Local module imports (relative to module root)
import (
    "github.com/codanna/testproject/internal/config"
    "github.com/codanna/testproject/pkg/utils"
)

// Relative imports (uncommon in Go but valid)
import (
    "./subpackage"
    "../common"
)

// Import aliases
import (
    json "encoding/json"
    mylog "log"
    . "math" // dot import
    _ "database/sql/driver" // blank import
)

// Vendor imports (these would be resolved from vendor/ directory)
import (
    "github.com/vendored/package"
    "company.com/internal/tool"
)

// Example functions using the imports
func main() {
    // Standard library usage
    fmt.Println("Hello, World!")
    cwd, _ := os.Getwd()
    base := filepath.Base(cwd)
    upper := strings.ToUpper(base)
    
    // External module usage
    router := gin.Default()
    db, _ := sql.Open("postgres", "connection_string")
    hash, _ := bcrypt.GenerateFromPassword([]byte("password"), bcrypt.DefaultCost)
    
    // Local module usage
    cfg := config.Load()
    result := utils.Process(data)
    
    // Relative import usage
    sub := subpackage.NewHandler()
    commonData := common.GetData()
    
    // Alias usage
    data, _ := json.Marshal(map[string]string{"key": "value"})
    mylog.Println("Logging message")
    pi := Pi // from math package via dot import
    
    // Vendor usage
    vendor := vendored.NewClient()
    tool := internal.GetTool()
}