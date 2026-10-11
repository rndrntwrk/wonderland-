const boot = document.getElementById('boot-state');
try {
  if (typeof WebAssembly !== 'object') throw new Error('This browser does not support WebAssembly.');
  const { default: init } = await import('./pkg/wonderland_creator_web.js');
  await init();
  boot?.remove();
} catch (error) {
  if (boot) {
    boot.setAttribute('role', 'alert');
    boot.textContent = 'The resource editor could not start. Reload this page after checking that its JavaScript and WebAssembly files are available.';
  }
  console.error('Creator startup failed', error);
}

window.addEventListener('beforeunload', event => {
  if (document.querySelector('[data-dirty="true"]')) {
    event.preventDefault();
    event.returnValue = '';
  }
});
