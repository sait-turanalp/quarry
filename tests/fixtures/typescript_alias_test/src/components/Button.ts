export class Button {
    constructor(public label: string) {}

    click(): void {
        console.log(`Button ${this.label} clicked`);
    }
}