// Test file for TypeScript import extraction
// This file contains all TypeScript import patterns for testing

// Named imports
import { Component, useState } from 'react';
import { Helper as H, util } from './utils/helper';

// Default imports
import React from 'react';
import Button from './components/Button';

// Namespace imports
import * as lodash from 'lodash';
import * as utils from '../utils';

// Mixed imports (default + named)
import DefaultExport, { namedExport } from './mixed';

// Type-only imports (TypeScript 3.8+)
import type { Props, State } from './types';
import { type Config, createConfig } from './config';

// Side-effect imports
import './styles.css';
import 'polyfill';

// Re-exports
export { Component } from 'react';
export * from './utils';
export { default as MyButton } from './Button';

// Re-export with rename
export { Helper as PublicHelper } from './utils/helper';

// Type re-exports
export type { Props } from './types';

// Path variations
import { something } from './sibling';
import { parent } from '../parent';
import { deep } from '../../deep/module';
import { indexed } from './folder'; // implies ./folder/index

// Node modules with scopes
import { Request } from '@types/express';
import { service } from '@app/services';

// Dynamic imports (for reference, not handled in phase 1)
const lazy = () => import('./lazy');

// Using the imports to avoid unused warnings
export function App(): React.FC<Props> {
    const [state, setState] = useState<State>();
    const config = createConfig();
    const helper = new H();
    lodash.debounce(() => {}, 100);
    
    return React.createElement(Button, { 
        config,
        state,
        util,
        DefaultExport,
        namedExport 
    });
}

// Class using imports
export class TestClass implements Props {
    private config: Config;
    
    constructor() {
        this.config = createConfig();
    }
    
    render() {
        return something();
    }
}