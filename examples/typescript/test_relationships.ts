// Test file for TypeScript relationship tracking

// Function calls
function greet(name: string): string {
    return formatMessage(name);
}

function formatMessage(text: string): string {
    return `Hello, ${text}!`;
}

// Method calls
class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
    
    calculate(): number {
        const result = this.add(5, 3);
        console.log(result);
        return result;
    }
}

// Imports (ES6)
import { Component } from '@angular/core';
import * as React from 'react';
import defaultExport from './module';

// CommonJS imports
const fs = require('fs');
const { readFile } = require('fs/promises');