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

function receiptMessage(receipt) {
  switch (receipt.phase) {
    case 'running': return `正在安装应用 ${receipt.epoch}，网页可能暂时断开。`;
    case 'confirmed': return `应用 ${receipt.epoch} 已安装并通过健康检查。`;
    case 'restored': return `应用 ${receipt.epoch} 未完成安装，设备已恢复原版本。`;
    default: return `已受理应用 ${receipt.epoch} 的安装请求，等待设备维护。`;
  }
}

export class UpdatePackage {
  constructor({document, getToken, request,
    confirm = message => globalThis.confirm(message),
    fetch = (url, options) => globalThis.fetch(url, options)}) {
    const find = name => document.getElementById(`update-package${name}`);
    this.file = find('');
    this.button = find('-preview');
    this.status = find('-state');
    this.result = find('-result');
    this.epoch = find('-epoch');
    this.schema = find('-schema');
    this.submitButton = find('-submit');
    this.queryButton = find('-query');
    this.discardButton = find('-discard');
    this.document = document;
    this.retentionPanel = find('-retention');
    this.retentionState = find('-retention-state');
    this.backupList = find('-backups');
    this.accepted = null;
    this.token = getToken;
    this.request = request;
    this.confirm = confirm;
    this.candidate = null;
    this.uncertain = false;
    this.fetch = fetch;
    this.controller = null;
    this.file.onchange = () => this.showSelection();
    this.button.onclick = () => this.preview();
    this.submitButton.onclick = () => this.submit();
    this.queryButton.onclick = () => this.query();
    this.discardButton.onclick = () => this.discard();
    this.reset();
  }

  reset() {
    this.controller?.abort();
    this.controller = null;
    this.uncertain = false;
    this.accepted = null;
    this.retention = null;
    this.backupDeletion = null;
    clearTimeout(this.backupRefresh);
    this.file.value = '';
    this.showSelection();
  }

  showSelection() {
    const file = this.file.files?.[0];
    const validFile = file && file.size > 0 && file.size <= MAX_BYTES && /\.rshapp$/i.test(file.name);
    this.candidate = null;
    this.result.hidden = true;
    this.epoch.textContent = '';
    this.schema.textContent = '';
    this.file.disabled = !this.token() || !!this.controller;
    this.button.disabled = this.file.disabled || !validFile;
    this.submitButton.hidden = true;
    this.submitButton.disabled = true;
    this.queryButton.disabled = !this.token() || !!this.controller;
    this.discardButton.hidden = !this.accepted;
    this.discardButton.disabled = this.queryButton.disabled || !this.accepted || this.accepted.phase === 'running';
    this.status.textContent = !this.token() ? '以所有者身份登录后可预览' :
      file && !validFile ? '请选择有效大小的 .rshapp 文件。' : '';
    this.renderRetention();
  }

  renderRetention() {
    if (!this.retentionPanel) return;
    this.retentionPanel.hidden = !this.retention || !this.token();
    if (this.retentionPanel.hidden) return;
    const {current, backups, canDelete, deleteHint} = this.retention;
    const pending = this.backupDeletion;
    const message = pending ? `正在核对应用 ${pending.epoch} 的删除结果。` :
      backups.length ? '最多保留两份备用应用。' : '暂无备用应用。';
    this.retentionState.textContent = `当前应用 ${current.epoch}。 ${message}${backups.length && !pending ? deleteHint : ''}`;
    this.backupList.replaceChildren(...backups.map(backup => {
      const row = this.document.createElement('li');
      row.textContent = `应用 ${backup.epoch} `;
      const remove = this.document.createElement('button');
      remove.type = 'button';
      remove.className = 'pg-btn';
      remove.textContent = `删除应用 ${backup.epoch}`;
      remove.disabled = !canDelete || !!this.controller || !!pending;
      remove.onclick = () => this.removeBackup(backup);
      row.append(remove);
      return row;
    }));
  }

  setRetention(value) {
    const retention = value?.retention;
    const valid = version => Number.isSafeInteger(version?.epoch) && version.epoch > 0
      && typeof version.manifest_sha256 === 'string' && /^[0-9a-f]{64}$/.test(version.manifest_sha256);
    if (retention?.format !== 1 || !valid(retention.current) || !Array.isArray(retention.backups)
      || retention.backups.length > 2 || !retention.backups.every((backup, index) => valid(backup)
        && backup.epoch < (index ? retention.backups[index - 1].epoch : retention.current.epoch)
        && backup.manifest_sha256 !== retention.current.manifest_sha256
        && (!index || backup.manifest_sha256 !== retention.backups[index - 1].manifest_sha256))) {
      this.retention = null;
      this.renderRetention();
      return;
    }
    const pending = this.backupDeletion;
    if (pending && (Date.now() >= pending.expires || !retention.backups.some(
      backup => backup.manifest_sha256 === pending.manifest_sha256))) {
      this.backupDeletion = null;
    }
    const receipt = value.receipt;
    const installing = receipt && !['confirmed','restored'].includes(receipt.phase);
    const canDelete = (value.can_delete_backup ?? value.can_submit) === true;
    this.retention = {...retention, canDelete: canDelete && !installing,
      deleteHint: installing ? '安装完成后可删除。' : canDelete ? '' : '删除需要最近五分钟内的所有者登录。'};
    this.renderRetention();
  }

  async removeBackup(backup) {
    if (this.controller || this.backupDeletion || !this.retention?.canDelete
      || !this.retention.backups.includes(backup)) return;
    if (!this.confirm(`永久删除备用应用 ${backup.epoch}？该版本不能再用于自动回退。`)) return;
    const token = this.token();
    const controller = new AbortController();
    this.controller = controller;
    this.backupDeletion = {...backup, expires: Date.now() + 91000};
    this.renderRetention();
    let message = '删除结果待核对，请查询安装请求。';
    try {
      const value = await this.request({op:'delete_update_backup', backup});
      if (value?.deletion_queued === true && value.epoch === backup.epoch) {
        message = `备用应用 ${backup.epoch} 的删除请求已受理。`;
      }
    } catch {
      // The request may have been published before the response was lost.
    } finally {
      if (this.controller === controller && token === this.token()) {
        this.controller = null;
        this.status.textContent = message;
        this.renderRetention();
        this.backupRefresh = setTimeout(() => void this.query(), 35000);
      }
    }
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
          candidate.epoch < 1 || candidate.schema_min < 0 || candidate.schema_max < candidate.schema_min ||
          !/^[0-9a-f]{64}$/.test(candidate.manifest_sha256)) {
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
            this.candidate = {...value, file};
            this.submitButton.hidden = this.uncertain;
            this.submitButton.disabled = this.uncertain;
            this.result.hidden = false;
            this.epoch.textContent = String(value.epoch);
            this.schema.textContent = `${value.schema_min}–${value.schema_max}`;
          }
        }
      }
    }
  }

  async submit() {
    if (this.controller || this.submitButton.disabled || !this.candidate || this.uncertain) return;
    const {file, epoch, manifest_sha256:manifest} = this.candidate;
    if (!this.confirm(`提交应用 ${epoch} 的安装请求？设备会保存已确认的更新包供维护程序处理。`)) return;
    const requestId = crypto.randomUUID();
    const token = this.token();
    const controller = new AbortController();
    this.controller = controller;
    this.showSelection();
    this.status.textContent = '正在提交确认过的签名包…';
    const timeout = setTimeout(() => controller.abort(), 85000);
    let text;
    try {
      const response = await this.fetch('/api/update/v1/submit', {
        method: 'POST',
        headers: {'Content-Type':'application/octet-stream', 'X-RustSimHub-Request':'2',
          'X-RustSimHub-Update-Request':requestId, 'X-RustSimHub-Update-Manifest':manifest,
          Authorization:`Bearer ${token}`},
        body:file, signal:controller.signal, credentials:'omit', cache:'no-store', redirect:'error'
      });
      if (response.status === 202) {
        const receipt = await response.json();
        if (receipt.state !== 'received' || receipt.request_id !== requestId ||
            receipt.manifest_sha256 !== manifest || receipt.epoch !== epoch) throw Error();
        text = `已受理应用 ${epoch} 的安装请求。`;
        if (this.controller === controller && token === this.token()) {
          this.accepted = receipt;
          this.uncertain = true;
        }
      } else if (response.status === 403) {
        text = '当前会话无权提交更新。请在主界面以所有者身份登录。';
      } else if (response.status === 409) {
        text = '已有请求或包内容已变化。请先查询安装请求。';
        if (this.controller === controller) this.uncertain = true;
      } else {
        throw Error();
      }
    } catch {
      if (this.controller === controller) this.uncertain = true;
      text = '提交结果未确认。请查询安装请求，避免重复提交。';
    } finally {
      clearTimeout(timeout);
      if (this.controller === controller) {
        this.controller = null;
        this.showSelection();
        if (token === this.token()) this.status.textContent = text;
      }
    }
  }

  async query() {
    if (this.controller || this.queryButton.disabled) return;
    const token = this.token();
    // The shared authenticated request already has its own short deadline.
    const controller = new AbortController();
    this.controller = controller;
    this.showSelection();
    this.status.textContent = '正在查询安装请求…';
    let value;
    try {
      value = await this.request({op:'update_upload_status'});
    } catch {
      value = null;
    } finally {
      if (this.controller === controller) {
        this.controller = null;
        this.showSelection();
        if (token === this.token()) {
          const receipt = value?.receipt;
          if (value?.available === true && receipt === null) {
            this.uncertain = false;
            this.accepted = null;
            this.showSelection();
            this.status.textContent = '暂无已受理的安装请求。';
          } else if (receipt?.state === 'received' && Number.isSafeInteger(receipt.epoch) && receipt.epoch > 0 &&
                     /^[0-9a-f-]{36}$/.test(receipt.request_id)) {
            this.uncertain = true;
            this.accepted = receipt;
            this.showSelection();
            this.status.textContent = receiptMessage(receipt);
          } else {
            this.status.textContent = '暂时无法查询安装请求。';
          }
          this.setRetention(value);
        }
      }
    }
  }


  async discard() {
    if (this.controller || this.discardButton.disabled || !this.accepted ||
        !this.confirm('移除这次上传的文件？已经暂存或运行的版本不受影响。')) return;
    const {request_id} = this.accepted;
    const token = this.token();
    const controller = new AbortController();
    this.controller = controller;
    this.showSelection();
    let text;
    try {
      const value = await this.request({op:'discard_update_upload', request_id});
      if (value?.discarded !== true || value.request_id !== request_id) throw Error();
      if (this.controller === controller && token === this.token()) {
        this.uncertain = false;
        this.accepted = null;
      }
      text = '已移除上传的文件。';
    } catch {
      text = '移除结果未确认。请使用主界面的所有者登录，并重新查询请求。';
    } finally {
      if (this.controller === controller) {
        this.controller = null;
        this.showSelection();
        if (token === this.token()) this.status.textContent = text;
      }
    }
  }

}
