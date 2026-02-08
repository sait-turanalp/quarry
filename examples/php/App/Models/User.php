<?php

namespace App\Models;

/**
 * User model
 */
class User extends Model
{
    protected string $table = 'users';
    
    protected array $fillable = ['name', 'email', 'password'];
    
    protected array $hidden = ['password'];
    
    public int $id;
    public string $name;
    public string $email;
    protected string $password;
    
    /**
     * Find user by ID
     */
    public static function find(int $id): ?self
    {
        // Mock implementation
        $user = new self();
        $user->id = $id;
        $user->name = 'John Doe';
        $user->email = 'john@example.com';
        return $user;
    }
    
    /**
     * Find user by email
     */
    public static function findByEmail(string $email): ?self
    {
        // Mock implementation
        $user = new self();
        $user->id = 1;
        $user->name = 'John Doe';
        $user->email = $email;
        return $user;
    }
    
    /**
     * Set password with hashing
     */
    public function setPassword(string $password): void
    {
        $this->password = password_hash($password, PASSWORD_DEFAULT);
    }
    
    /**
     * Verify password
     */
    public function verifyPassword(string $password): bool
    {
        return password_verify($password, $this->password);
    }
    
    /**
     * Convert to array for serialization
     */
    public function toArray(): array
    {
        $data = [
            'id' => $this->id,
            'name' => $this->name,
            'email' => $this->email,
        ];
        
        // Remove hidden fields
        foreach ($this->hidden as $field) {
            unset($data[$field]);
        }
        
        return $data;
    }
}