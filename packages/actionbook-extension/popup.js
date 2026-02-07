// Popup script - displays connection status

function updateUI(state) {
  const bridgeDot = document.getElementById("bridgeDot");
  const bridgeStatus = document.getElementById("bridgeStatus");
  const tabDot = document.getElementById("tabDot");
  const tabStatus = document.getElementById("tabStatus");

  switch (state.connectionState) {
    case "connected":
      bridgeDot.className = "dot green";
      bridgeStatus.textContent = "Connected";
      break;
    case "connecting":
      bridgeDot.className = "dot yellow";
      bridgeStatus.textContent = "Connecting...";
      break;
    default:
      bridgeDot.className = "dot red";
      bridgeStatus.textContent = "Disconnected";
  }

  if (state.attachedTabId) {
    tabDot.className = "dot green";
    tabStatus.textContent = `Tab #${state.attachedTabId}`;
  } else {
    tabDot.className = "dot gray";
    tabStatus.textContent = "No tab attached";
  }
}

// Get initial state
chrome.runtime.sendMessage({ type: "getState" }, (response) => {
  if (response) updateUI(response);
});

// Listen for state updates
chrome.runtime.onMessage.addListener((message) => {
  if (message.type === "stateUpdate") {
    updateUI(message);
  }
});
