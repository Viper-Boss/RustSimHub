// SPDX-License-Identifier: AGPL-3.0-only
const MAX_BYTES = 24 * 1024 * 1024 + 8192 + 76;
const RECHECK_MS = 3000;
const SUBMIT_WINDOW_MS = 65000;
const WATCH_MS = 480000;
const HASH = /^[0-9a-f]{64}$/;
const REQUEST = /^[0-9a-f-]{36}$/;
const messages = {
  400: '更新包读取失败，请重新选择文件。',
  401: '请以所有者身份登录。',
  403: '请以所有者身份登录。',
  408: '传输超时，请重试。',
  413: '更新包超过大小限制。',
  422: '更新包验证失败，请核对文件和设备型号。',
  429: '设备正在处理另一个请求，请稍后重试。'
};

function terminal(receipt) {
  return receipt?.phase === 'confirmed' || receipt?.phase === 'restored';
}

function receiptText(receipt) {
  const name = '固件 ' + receipt.epoch;
  if (receipt.phase === 'confirmed') return name + ' 已安装并通过健康检查。';
  if (receipt.phase === 'restored') return name + ' 未完成安装，设备已恢复原版本。';
  if (receipt.phase === 'running') return '正在安装' + name + '，网页可能暂时断开。';
  return '已受理' + name + '，正在等待设备安装。';
}

function validFile(file) {
  return file && file.size > 0 && file.size <= MAX_BYTES && /\.rshapp$/i.test(file.name);
}

function validVersion(value) {
  return Number.isSafeInteger(value?.epoch) && value.epoch > 0 && HASH.test(value.manifest_sha256);
}

export class UpdatePackage {
  constructor({
    document, getToken, request, reconnect,
    confirm = message => globalThis.confirm(message),
    fetch = (url, options) => globalThis.fetch(url, options),
    now = Date.now,
    schedule = (callback, delay) => globalThis.setTimeout(callback, delay),
    cancel = timer => globalThis.clearTimeout(timer)
  }) {
    const find = name => document.getElementById('update-package' + name);
    this.file = find('');
    this.previewButton = find('-preview');
    this.status = find('-state');
    this.result = find('-result');
    this.epoch = find('-epoch');
    this.schema = find('-schema');
    this.submitButton = find('-submit');
    this.queryButton = find('-query');
    this.progress = find('-progress');
    this.receiptLine = find('-receipt');
    this.currentVersion = find('-current');
    this.retentionPanel = find('-retention');
    this.retentionState = find('-retention-state');
    this.backupList = find('-backups');
    this.document = document;
    this.token = getToken;
    this.request = request;
    this.reconnect = reconnect;
    this.confirm = confirm;
    this.fetch = fetch;
    this.now = now;
    this.schedule = schedule;
    this.cancel = cancel;
    this.file.onchange = () => {
      this.candidate = null;
      this.message = '';
      this.render();
      return this.preview();
    };
    this.previewButton.onclick = () => this.preview();
    this.submitButton.onclick = () => this.submit();
    this.queryButton.onclick = () => this.query();
    this.reset();
  }

  reset() {
    this.action?.controller.abort();
    this.cancel(this.pollTimer);
    this.pollTimer = null;
    this.action = this.lookup = this.candidate = this.pending = this.receipt = null;
    this.retention = this.backupDeletion = null;
    this.file.value = '';
    this.message = '';
    this.watchUntil = 0;
    this.reconnected = false;
    this.canSubmit = true;
    this.render();
  }

  waiting() {
    return !!this.pending || (!!this.receipt && !terminal(this.receipt));
  }

  syncingRetention() {
    return this.now() < this.watchUntil && this.receipt?.phase === 'confirmed'
      && (!this.retention || this.retention.current.epoch < this.receipt.epoch);
  }

  render() {
    const signedIn = !!this.token();
    const waiting = this.waiting();
    const busy = waiting || !!this.action;
    const chosen = this.file.files?.[0];
    const version = this.candidate?.epoch;
    const floor = Math.max(this.retention?.current.epoch ?? 0, this.receipt?.epoch ?? 0);
    const tooOld = !!version && version <= floor;
    this.file.disabled = !signedIn || busy;
    this.previewButton.hidden = !validFile(chosen) || !!version || busy;
    this.previewButton.disabled = !signedIn || busy;
    this.submitButton.disabled = !signedIn || busy || !version || tooOld || !this.canSubmit;
    this.submitButton.textContent = version && !tooOld ? '安装 ' + version : '安装更新';
    this.queryButton.disabled = !signedIn || !!this.action || !!this.lookup;
    this.progress.hidden = !signedIn || !busy;
    this.result.hidden = !version;
    this.epoch.textContent = version ? String(version) : '';
    this.schema.textContent = version ? this.candidate.schema_min + '–' + this.candidate.schema_max : '';
    this.status.textContent = !signedIn ? '以所有者身份登录后即可更新。'
      : this.message || (waiting ? '设备正在处理更新，页面会自动查看结果。'
      : !this.canSubmit ? '当前会话没有更新权限。'
      : tooOld ? '请选择版本高于 ' + floor + ' 的更新包。'
      : version ? '固件 ' + version + ' 已验证，可以安装。'
      : chosen && !validFile(chosen) ? '请选择有效的 .rshapp 更新包。'
      : '选择更新包后自动检查。');
    this.receiptLine.hidden = !signedIn || !this.receipt;
    this.receiptLine.textContent = this.receipt ? receiptText(this.receipt) : '';
    this.renderRetention();
  }

  setRetention(value) {
    const retention = value?.retention;
    if (retention?.format !== 1 || !validVersion(retention.current) || !Array.isArray(retention.backups)
      || retention.backups.length > 2 || !retention.backups.every((backup, index) => validVersion(backup)
        && backup.epoch < (index ? retention.backups[index - 1].epoch : retention.current.epoch)
        && backup.manifest_sha256 !== retention.current.manifest_sha256
        && (!index || backup.manifest_sha256 !== retention.backups[index - 1].manifest_sha256))) {
      this.retention = null;
      return;
    }
    if (this.backupDeletion) {
      const {epoch, manifest_sha256, expires} = this.backupDeletion;
      const present = retention.backups.some(backup => backup.manifest_sha256 === manifest_sha256);
      if (!present || this.now() >= expires) {
        this.backupDeletion = null;
        this.message = present ? `固件 ${epoch} 仍在备用列表，请刷新状态。` : `已删除固件 ${epoch}。`;
      }
    }
    this.retention = {...retention, canDelete: (value.can_delete_backup ?? value.can_submit) === true};
  }

  renderRetention() {
    if (this.token() && this.syncingRetention()) {
      this.currentVersion.hidden = false;
      this.currentVersion.textContent = '正在核对固件版本';
      this.retentionPanel.hidden = true;
      return;
    }
    const show = !!this.retention && !!this.token();
    this.currentVersion.hidden = this.retentionPanel.hidden = !show;
    if (!show) return;
    const {current, backups, canDelete} = this.retention;
    this.currentVersion.textContent = '当前 ' + current.epoch;
    const pending = this.backupDeletion;
    this.retentionState.textContent = pending ? '正在核对备用固件 ' + pending.epoch + ' 的删除结果。'
      : !backups.length ? '暂无备用固件。'
      : !canDelete ? '请以所有者身份登录后删除备用固件。' : '备用固件';
    this.backupList.replaceChildren(...backups.map(backup => {
      const row = this.document.createElement('li');
      row.textContent = '固件 ' + backup.epoch;
      const remove = this.document.createElement('button');
      remove.type = 'button';
      remove.className = 'pg-btn danger';
      remove.textContent = '删除固件 ' + backup.epoch;
      remove.disabled = !canDelete || !!this.action || this.waiting() || !!pending;
      remove.onclick = () => this.removeBackup(backup);
      row.append(remove);
      return row;
    }));
  }

  begin(message) {
    const action = {token: this.token(), controller: new AbortController()};
    this.action = action;
    this.lookup = null;
    this.message = message;
    this.render();
    return action;
  }

  async upload(endpoint, action, file, headers = {}) {
    const timer = globalThis.setTimeout(() => action.controller.abort(), 85000);
    try {
      return await this.fetch('/api/update/v1/' + endpoint, {
        method: 'POST', body: file, credentials: 'omit', cache: 'no-store', redirect: 'error',
        signal: action.controller.signal,
        headers: {'Content-Type': 'application/octet-stream', 'X-RustSimHub-Request': '2',
          Authorization: 'Bearer ' + action.token, ...headers}
      });
    } finally {
      globalThis.clearTimeout(timer);
    }
  }

  async preview() {
    const file = this.file.files?.[0];
    if (!this.token() || this.action || this.waiting() || !validFile(file)) return;
    const action = this.begin('正在检查固件包…');
    try {
      const response = await this.upload('preview', action, file);
      if (!response.ok) throw Error(messages[response.status] || '暂时无法检查更新包，请重试。');
      const value = await response.json();
      if (value?.verified !== true || !validVersion(value) ||
          !Number.isSafeInteger(value.schema_min) || !Number.isSafeInteger(value.schema_max) ||
          value.schema_min < 0 || value.schema_max < value.schema_min) {
        throw Error('设备返回的更新包信息无效。');
      }
      if (this.action === action) {
        this.candidate = {...value, file};
        this.message = '';
      }
    } catch (error) {
      if (this.action === action) this.message = error.name === 'AbortError' ? '检查超时，请重试。'
        : error instanceof TypeError || error instanceof SyntaxError ? '暂时无法检查更新包，请重试。'
        : error.message;
    } finally {
      if (this.action === action) {
        this.action = null;
        this.render();
      }
    }
  }

  async submit() {
    if (this.submitButton.disabled || this.action || !this.candidate) return;
    const {file, epoch, manifest_sha256: manifest} = this.candidate;
    if (!this.confirm('安装固件 ' + epoch + '？安装期间网页会短暂断开。')) return;
    const id = crypto.randomUUID();
    const action = this.begin('正在提交固件 ' + epoch + '…');
    this.reconnected = false;
    try {
      const response = await this.upload('submit', action, file, {
        'X-RustSimHub-Update-Request': id, 'X-RustSimHub-Update-Manifest': manifest
      });
      if (this.action !== action) return;
      if (response.status === 202) {
        const receipt = await response.json();
        if (this.action !== action) return;
        if (receipt.state !== 'received' || receipt.request_id !== id ||
            receipt.manifest_sha256 !== manifest || receipt.epoch !== epoch) throw Error();
        this.receipt = receipt;
        this.watchUntil = this.now() + WATCH_MS;
        this.message = '';
      } else if (response.status === 409) {
        this.message = '设备有其他请求或包已变化，正在刷新状态。';
      } else if (messages[response.status]) {
        this.message = messages[response.status];
      } else {
        throw Error();
      }
    } catch {
      if (this.action === action) {
        this.pending = {id, deadline: this.now() + SUBMIT_WINDOW_MS};
        this.watchUntil = this.now() + WATCH_MS;
        this.message = '提交结果待确认，正在自动核对。';
      }
    } finally {
      if (this.action === action) {
        this.action = null;
        this.render();
        if (this.waiting() || this.message.includes('正在刷新')) void this.query();
      }
    }
  }

  async query() {
    if (!this.token() || this.lookup || this.action) return;
    const lookup = {};
    this.lookup = lookup;
    this.cancel(this.pollTimer);
    this.pollTimer = null;
    this.render();
    try {
      let value;
      try {
        value = await this.request({op: 'update_upload_status'});
      } catch (error) {
        const denied = error?.httpStatus === 401 || error?.httpStatus === 403 || error?.coreCode === 'denied';
        if (this.lookup !== lookup || !denied || !this.waiting() || this.reconnected || !this.reconnect) throw error;
        this.reconnected = true;
        await this.reconnect();
        if (this.lookup !== lookup) return;
        value = await this.request({op: 'update_upload_status'});
      }
      if (this.lookup !== lookup) return;
      if (value?.available !== true) throw Error();
      const receipt = value.receipt;
      if (receipt !== null && (receipt?.state !== 'received' || !validVersion(receipt) ||
          !REQUEST.test(receipt.request_id))) throw Error();
      this.canSubmit = value.can_submit !== false;
      const firstReceipt = receipt && this.receipt?.request_id !== receipt.request_id;
      this.receipt = receipt;
      this.setRetention(value);
      if (firstReceipt && !terminal(receipt)) this.watchUntil = this.now() + WATCH_MS;
      if (this.pending && (receipt?.request_id === this.pending.id || this.now() >= this.pending.deadline)) {
        this.message = receipt?.request_id === this.pending.id ? '' : '设备未确认这次提交，请重新核对后安装。';
        this.pending = null;
      } else if (!this.pending && (!this.message || this.message.startsWith('正在等待') ||
          this.message.startsWith('暂时无法读取') || this.message.includes('正在刷新'))) {
        this.message = '';
      }
      if (receipt?.phase === 'confirmed' && this.candidate?.manifest_sha256 === receipt.manifest_sha256) {
        this.candidate = null;
        this.file.value = '';
      }
    } catch {
      if (this.lookup === lookup) this.message = this.waiting()
        ? '正在等待设备恢复连接，恢复后会自动核对安装结果。' : '暂时无法读取设备的更新状态。';
    } finally {
      if (this.lookup === lookup) {
        this.lookup = null;
        if (this.backupDeletion && this.now() >= this.backupDeletion.expires) {
          this.message = `固件 ${this.backupDeletion.epoch} 的删除结果待核对，请刷新状态。`;
          this.backupDeletion = null;
        }
        if ((this.waiting() && this.now() < this.watchUntil) || this.syncingRetention() || this.backupDeletion) {
          this.pollTimer = this.schedule(() => void this.query(), RECHECK_MS);
        } else if (this.waiting()) {
          this.message = '设备尚未返回最终结果，可以在「版本与恢复」里刷新状态。';
        }
        this.render();
      }
    }
  }

  async removeBackup(backup) {
    if (this.action || this.waiting() || this.syncingRetention() || this.backupDeletion || !this.retention?.canDelete
      || !this.retention.backups.includes(backup)) return;
    if (!this.confirm('永久删除备用固件 ' + backup.epoch + '？')) return;
    const action = this.begin('正在请求删除备用固件…');
    this.backupDeletion = {...backup, expires: this.now() + 91000};
    this.render();
    try {
      const value = await this.request({op: 'delete_update_backup', backup});
      if (this.action === action && value?.deletion_queued === true && value.epoch === backup.epoch) {
        this.message = '备用固件 ' + backup.epoch + ' 的删除请求已受理。';
      }
    } catch (error) {
      if (this.action === action) {
        if (error?.coreCode) {
          this.backupDeletion = null;
          this.message = error.coreCode === 'denied' ? '请以所有者身份登录后删除备用固件。'
            : '设备未受理删除，请刷新版本列表后再试。';
        } else {
          this.message = '正在核对备用固件的删除结果。';
        }
      }
    } finally {
      if (this.action === action) {
        this.action = null;
        this.render();
        this.pollTimer = this.schedule(() => void this.query(), RECHECK_MS);
      }
    }
  }
}
