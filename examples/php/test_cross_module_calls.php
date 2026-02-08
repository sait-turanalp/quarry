<?php
/**
 * Test file for cross-module call tracking in PHP
 */

namespace App\Config;

use App\Utils\Helper;
use App\Services\Auth\UserManager;

class ConfigManager {
    /**
     * Initialize configuration file
     */
    public function initConfigFile() {
        // This should be tracked as calling App\Init\GlobalDirs::initGlobalDirs
        \App\Init\GlobalDirs::initGlobalDirs();

        // This should be tracked as calling Helper::processData
        Helper::processData();

        // This should be tracked as calling UserManager::createUser
        UserManager::createUser("test@example.com");
    }

    /**
     * Initialize global directories (local method)
     */
    public function initGlobalDirs() {
        echo "Initializing global directories locally\n";
    }

    /**
     * Process local data
     */
    public function processLocalData() {
        // This calls the local method
        $this->initGlobalDirs();

        // This calls the static method from another namespace
        \App\Init\GlobalDirs::initGlobalDirs();
    }
}