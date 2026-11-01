// Runs inside the owned native WKWebView. Only synthetic tray IPC and this page are used.
(async () => {
  const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
  const check = (ok, message) => { if (!ok) throw new Error(message); };
  const reports = [];
  const measure = step => {
    const rect = selector => document.querySelector(selector).getBoundingClientRect();
    const head = rect('.head'), foot = rect('.foot'), body = rect('.body:not([hidden])');
    const report = { step, root: document.scrollingElement.scrollTop, head: head.top, footer: foot.bottom, height: innerHeight, contentTop: body.top, contentBottom: body.bottom };
    reports.push(report);
    check(report.root === 0, `${step}: document scrolled ${report.root}px`);
    check(Math.abs(head.top) <= 1, `${step}: header moved to ${head.top}`);
    check(Math.abs(foot.bottom - innerHeight) <= 1, `${step}: gap below footer ${innerHeight - foot.bottom}px`);
    check(Math.abs(body.bottom - foot.top) <= 1, `${step}: body overlaps footer`);
    check(document.documentElement.scrollWidth === innerWidth, `${step}: horizontal overflow`);
  };
  try {
    for (let i = 0; i < 100 && !document.querySelector('.row'); i++) await wait(50);
    check(document.querySelector('.row'), 'Tray never loaded');
    await document.fonts.ready;
    measure('initial');
    for (const [i, section] of ['agents', 'needs', 'dev', 'cpu', 'top', 'agents'].entries()) {
      window.panelData.focus = section;
      window.panelData.focusSeq = i + 1;
      for (let attempt = 0; attempt < 100 && window.seenFocusSeq !== i + 1; attempt++) await wait(50);
      check(window.seenFocusSeq === i + 1, `${section}: click was not delivered`);
      await wait(100);
      measure(section);
      const scroll = document.querySelector('.body').scrollTop;
      await wait(1100);
      check(Math.abs(document.querySelector('.body').scrollTop - scroll) <= 1, `${section}: polling moved the list from ${scroll} to ${document.querySelector('.body').scrollTop}`);
      document.querySelector('#quick-settings-toggle').click();
      await wait(60);
      measure(`${section}:settings`);
      check(document.querySelector('.settings'), 'Settings did not open');
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await wait(60);
      measure(`${section}:back`);
      check(document.activeElement.id === 'quick-settings-toggle', 'Escape lost keyboard focus');
    }
    // Keyboard focus is another way browsers implicitly scroll ancestors.
    for (const button of document.querySelectorAll('.body:not([hidden]) button')) { button.focus(); measure('keyboard'); }
    document.activeElement?.blur();
    await wait(100);
    document.querySelector('.body').scrollTop = 0;
    await wait(100);
    measure('final');
    window.webkit.messageHandlers.fixture.postMessage({ status: 'passed', engine: navigator.userAgent, reports });
  } catch (error) {
    window.webkit.messageHandlers.fixture.postMessage({ status: 'failed', error: String(error), reports });
  }
})();
