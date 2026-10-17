export function render(el: { innerHTML: string }, userInput: string): void {
  el.innerHTML = userInput;
}
