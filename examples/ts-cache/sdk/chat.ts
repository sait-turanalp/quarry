export class ChatSDK {
  createChat(name: string = "test"): string {
    return `chat:${name}`;
  }
}

export function createChat(): string {
  return new ChatSDK().createChat("from_fn");
}

export default ChatSDK;

