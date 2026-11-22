// SPDX-License-Identifier: AGPL-3.0-only
const MAX_BYTES = 24 * 1024 * 1024 + 8192 + 76;
const MESSAGES = {
  401: '请以所有者身份登录后再预览。',
  403: '请重新验证所有者身份后再预览。',
  408: '上传超时，请手动重试。',
  413: '更新包超过大小限制。',
  422: '验签失败，请核对设备型号和更新包来源。',
  429: '已有验签预览在进行，请稍后重试。',
  503: '暂时无法验签，请稍后重试。'
};

export class UpdatePackage {
  constructor({document, getToken, fetch = globalThis.fetch}) {
    const find = name => document.getElementById(`update-package${name}`);
    this.file = find('');
    this.button = find('-preview');
    this.status = find('-state');
    this.result = find('-result');
    this.epoch = find('-epoch');
    this.schema = find('-schema');
    this.token = getToken;
    this.fetch = fetch;
    this.controller = null;
    this.file.onchange = () => this.showSelection();
    this.button.onclick = () => this.preview();
    this.reset();
  }

  reset() {
    this.controller?.abort();
    this.controller = null;
    this.file.value = '';
    this.showSelection();
  }

  showSelection() {
    const file = this.file.files?.[0];
    const validFile = file && file.size > 0 && file.size <= MAX_BYTES && /\.rshapp$/i.test(file.name);
    this.result.hidden = true;
    this.epoch.textContent = '';
    this.schema.textContent = '';
    this.file.disabled = !this.token() || !!this.controller;
    this.button.disabled = this.file.disabled || !validFile;
    this.status.textContent = !this.token() ? '以所有者身份登录后可预览' :
      file && !validFile ? '请选择有效大小的 .rshapp 文件。' : '';
  }

  async preview() {
    if (this.button.disabled || this.controller) return;
    const file = this.file.files[0];
    const token = this.token();
    const controller = new AbortController();
    this.controller = controller;
    this.showSelection();
    this.status.textContent = '正在上传并验签…';
    const deadline = setTimeout(() => controller.abort(), 85000);
    const current = () => this.controller === controller && token === this.token();
    let message = '';
    let value;
    try {
      const reply = await this.fetch('/api/update/v1/preview', {
        method: 'POST',
        headers: {'Content-Type': 'application/octet-stream', 'X-RustSimHub-Request': '2', Authorization: `Bearer ${token}`},
        body: file, signal: controller.signal, credentials: 'omit', cache: 'no-store', redirect: 'error'
      });
      if (!reply.ok) throw Error(MESSAGES[reply.status] || '暂时无法验签，请稍后重试。');
      const candidate = await reply.json();
      if (candidate?.verified !== true || ![candidate.epoch, candidate.schema_min, candidate.schema_max].every(Number.isSafeInteger) ||
          candidate.epoch < 1 || candidate.schema_min < 0 || candidate.schema_max < candidate.schema_min) {
        throw Error('验签回复无效，请稍后重试。');
      }
      value = candidate;
      message = '签名与载荷校验通过。安装资格将在安装前另行核验。';
    } catch (error) {
      message = error.name === 'AbortError' ? '上传超时，请手动重试。' :
        error instanceof TypeError || error instanceof SyntaxError ? '暂时无法验签，请稍后重试。' : error.message;
    } finally {
      clearTimeout(deadline);
      if (this.controller === controller) {
        const show = current();
        this.controller = null;
        this.showSelection();
        if (show) {
          this.status.textContent = message;
          if (value) {
            this.result.hidden = false;
            this.epoch.textContent = String(value.epoch);
            this.schema.textContent = `${value.schema_min}–${value.schema_max}`;
          }
        }
      }
    }
  }
}
