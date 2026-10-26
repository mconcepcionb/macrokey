// Evaluate an expression in the macrokey settings WebView over CDP.
// Requires the app to run with:
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
//
//   node tools/cdp_eval.mjs "Object.keys(doc.value.contexts)"

const expression = process.argv[2];
const wanted = process.argv[3] || "settings";
if (!expression) {
  console.error("usage: node tools/cdp_eval.mjs <expression> [settings|osd]");
  process.exit(2);
}

const list = await (await fetch("http://127.0.0.1:9222/json")).json();
const target = list.find((entry) => entry.title.toLowerCase().includes(wanted));
if (!target) {
  console.error(`target "${wanted}" not found`);
  process.exit(1);
}

const socket = new WebSocket(target.webSocketDebuggerUrl);
const pending = new Map();
let sequence = 0;

socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
};

await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

function send(method, params = {}) {
  return new Promise((resolve) => {
    const id = ++sequence;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
}

await send("Runtime.enable");
const response = await send("Runtime.evaluate", {
  expression,
  awaitPromise: true,
  returnByValue: true,
  userGesture: true,
});

if (response.result?.exceptionDetails) {
  console.error(JSON.stringify(response.result.exceptionDetails, null, 2));
  process.exit(1);
}
console.log(JSON.stringify(response.result?.result?.value ?? null, null, 2));
socket.close();
