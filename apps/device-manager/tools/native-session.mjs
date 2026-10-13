// CDP can be ready before WebView2 has published a page target. Never create a substitute page.
export async function waitForAppPage(browser, {attempts = 200, wait = () => new Promise(resolve => setTimeout(resolve,100)), check = () => {}} = {}) {
  for (let i=0;i<attempts;i++) {
    check();
    const page = browser.contexts().flatMap(context => context.pages()).find(page => {
      try { return new URL(page.url()).hostname === 'tauri.localhost'; } catch { return false; }
    });
    if (page) return page;
    await wait();
  }
  throw Error('WebView2 did not publish the application page within the bounded startup wait');
}
