// Trunk supplies the built WASM size, so progress also works with HTTP compression.
export default function initializer() {
  const loading = document.getElementById('loading');
  const status = document.getElementById('loading-status');
  const progress = document.getElementById('loading-progress');
  const detail = document.getElementById('loading-detail');
  const mebibytes = (bytes) => `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;

  return {
    onStart() {
      if (loading.dataset.failed) return;
      status.textContent = 'Downloading the orbital laboratory…';
      progress.removeAttribute('value');
      detail.textContent = 'Connecting…';
    },
    onProgress({ current, total }) {
      if (loading.dataset.failed) return;
      if (total > 0) {
        const percent = Math.min(100, Math.floor((current / total) * 100));
        progress.value = percent;
        detail.textContent = `${percent}% · ${mebibytes(current)} / ${mebibytes(total)}`;
        if (current >= total) {
          status.textContent = 'Initializing the orbital laboratory…';
          detail.textContent = `Download complete · ${mebibytes(total)}`;
        }
      } else {
        progress.removeAttribute('value');
        detail.textContent = `${mebibytes(current)} downloaded`;
      }
    },
    onSuccess() {
      if (loading.dataset.failed) return;
      status.textContent = 'Starting the renderer…';
      progress.removeAttribute('value');
      detail.textContent = 'Preparing your first view';
      // The existing Bevy scene lifecycle hides the overlay when its UI is ready.
    },
    onFailure(error) {
      window.dispatchEvent(new CustomEvent('three-body-load-error', { detail: error }));
    },
  };
}
