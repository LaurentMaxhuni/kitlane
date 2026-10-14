document.documentElement.classList.add('js');
const menu = document.querySelector('.menu-toggle');
const navigation = document.querySelector('#navigation');
menu?.addEventListener('click', () => {
  const expanded = menu.getAttribute('aria-expanded') !== 'true';
  menu.setAttribute('aria-expanded', String(expanded));
  navigation.classList.toggle('is-open', expanded);
});
function closeMenu() {
  menu?.setAttribute('aria-expanded', 'false');
  navigation?.classList.remove('is-open');
}
navigation?.addEventListener('click', event => {
  if (event.target.closest('a')) closeMenu();
});
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && menu?.getAttribute('aria-expanded') === 'true') {
    closeMenu();
    menu.focus();
  }
});
const previews = {
  library: { src: 'assets/library.png', alt: 'Kitlane Library showing installed apps, publishers, versions, and availability for Windows, macOS, and Linux', description: 'Everything installed, in one view.', caption: 'Library preview with illustrative inventory.' },
  devices: { src: 'assets/devices.png', alt: 'Kitlane Devices showing a paired computer and controls for explicitly syncing libraries', description: 'Your computers, connected on your terms.', caption: 'Devices preview with an illustrative paired computer.' }
};
document.querySelectorAll('[data-preview]').forEach(button => {
  button.addEventListener('click', () => {
    const preview = previews[button.dataset.preview];
    const image = document.querySelector('#product-image');
    image.src = preview.src;
    image.alt = preview.alt;
    document.querySelector('#product-caption').textContent = preview.caption;
    document.querySelector('#preview-description').textContent = preview.description;
    document.querySelectorAll('[data-preview]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
  });
});
document.querySelectorAll('[data-year]').forEach(element => { element.textContent = new Date().getFullYear(); });
