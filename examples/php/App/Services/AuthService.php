<?php

namespace App\Services;

use App\Models\User;

/**
 * Authentication service
 */
class AuthService
{
    private array $sessions = [];
    
    /**
     * Authenticate user with credentials
     */
    public function authenticate(string $email, string $password): ?string
    {
        $user = User::findByEmail($email);
        
        if (!$user || !$user->verifyPassword($password)) {
            return null;
        }
        
        // Generate token
        $token = bin2hex(random_bytes(32));
        $this->sessions[$token] = $user->id;
        
        return $token;
    }
    
    /**
     * Validate session token
     */
    public function validateToken(string $token): ?User
    {
        if (!isset($this->sessions[$token])) {
            return null;
        }
        
        $userId = $this->sessions[$token];
        return User::find($userId);
    }
    
    /**
     * Logout user by token
     */
    public function logout(string $token): bool
    {
        if (isset($this->sessions[$token])) {
            unset($this->sessions[$token]);
            return true;
        }
        
        return false;
    }
    
    /**
     * Check if user has permission
     */
    public function hasPermission(User $user, string $permission): bool
    {
        // Mock permission check
        return $user->id === 1; // Admin user
    }
}