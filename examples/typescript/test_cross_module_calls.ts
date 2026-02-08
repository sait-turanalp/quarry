/**
 * Test file for cross-module call tracking in TypeScript
 */

export function initConfigFile(): void {
    // This should be tracked as calling app.init.initGlobalDirs
    app.init.initGlobalDirs();
}

export function initGlobalDirs(): void {
    console.log("Initializing global directories");
}

// Simulated module namespace
namespace app {
    export namespace init {
        export function initGlobalDirs(): void {
            console.log("Initializing from app.init module");
        }
    }
}