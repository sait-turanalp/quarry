import ChatSDK, { createChat } from './sdk/chat';

export function startVoiceConversation() {
  const sdk = new ChatSDK();
  sdk.createChat('voice');
  createChat();
}

