// Test TypeScript implementations tracking
// Tests: extends, implements, interface extension

interface Serializable {
    serialize(): string;
}

interface Comparable<T> {
    compareTo(other: T): number;
}

// Interface extending another interface
interface AdvancedSerializable extends Serializable {
    deserialize(data: string): void;
}

// Multiple interface extension
interface Entity extends Serializable, Comparable<Entity> {
    id: string;
}

// Base class
class BaseEntity {
    id: string;
    constructor(id: string) {
        this.id = id;
    }
}

// Class extending and implementing
class User extends BaseEntity implements Serializable, Comparable<User> {
    name: string;
    
    constructor(id: string, name: string) {
        super(id);
        this.name = name;
    }
    
    serialize(): string {
        return JSON.stringify({ id: this.id, name: this.name });
    }
    
    compareTo(other: User): number {
        return this.name.localeCompare(other.name);
    }
}

// Class extending another class
class Admin extends User {
    permissions: string[];
    
    constructor(id: string, name: string, permissions: string[]) {
        super(id, name);
        this.permissions = permissions;
    }
}

// Abstract class with implementation
abstract class Shape implements Serializable {
    abstract area(): number;
    
    serialize(): string {
        return `Shape with area: ${this.area()}`;
    }
}

// Concrete implementation
class Circle extends Shape {
    radius: number;
    
    constructor(radius: number) {
        super();
        this.radius = radius;
    }
    
    area(): number {
        return Math.PI * this.radius * this.radius;
    }
}