<?php

namespace App\Controllers;

use App\Models\User;
use App\Services\AuthService;
use App\Http\Request;
use App\Http\Response;

/**
 * User controller for handling user-related requests
 */
class UserController extends BaseController
{
    private AuthService $authService;
    
    public function __construct(AuthService $authService)
    {
        $this->authService = $authService;
    }
    
    /**
     * Display user profile
     */
    public function show(Request $request, int $id): Response
    {
        $user = User::find($id);
        
        if (!$user) {
            return $this->notFound('User not found');
        }
        
        return $this->json($user->toArray());
    }
    
    /**
     * Create a new user
     */
    public function create(Request $request): Response
    {
        $data = $request->validated();
        
        $user = new User();
        $user->name = $data['name'];
        $user->email = $data['email'];
        $user->save();
        
        return $this->created($user->toArray());
    }
    
    /**
     * Update user information
     */
    public function update(Request $request, int $id): Response
    {
        $user = User::find($id);
        
        if (!$user) {
            return $this->notFound('User not found');
        }
        
        $user->fill($request->validated());
        $user->save();
        
        return $this->json($user->toArray());
    }
    
    /**
     * Delete a user
     */
    public function delete(int $id): Response
    {
        $user = User::find($id);
        
        if (!$user) {
            return $this->notFound('User not found');
        }
        
        $user->delete();
        
        return $this->noContent();
    }
    
    /**
     * Login a user
     */
    public function login(Request $request): Response
    {
        $credentials = $request->only(['email', 'password']);
        
        $token = $this->authService->authenticate(
            $credentials['email'],
            $credentials['password']
        );
        
        if (!$token) {
            return $this->unauthorized('Invalid credentials');
        }
        
        return $this->json(['token' => $token]);
    }
}