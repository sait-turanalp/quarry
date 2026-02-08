<?php

namespace App\Controllers;

use App\Http\Response;

/**
 * Base controller class
 */
abstract class BaseController
{
    /**
     * Return JSON response
     */
    protected function json(array $data, int $status = 200): Response
    {
        return new Response($data, $status);
    }
    
    /**
     * Return created response
     */
    protected function created(array $data): Response
    {
        return $this->json($data, 201);
    }
    
    /**
     * Return no content response
     */
    protected function noContent(): Response
    {
        return new Response(null, 204);
    }
    
    /**
     * Return not found response
     */
    protected function notFound(string $message = 'Not found'): Response
    {
        return $this->json(['error' => $message], 404);
    }
    
    /**
     * Return unauthorized response
     */
    protected function unauthorized(string $message = 'Unauthorized'): Response
    {
        return $this->json(['error' => $message], 401);
    }
}