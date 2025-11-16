/**
 * MCP-RS Web UI 共通JavaScript ライブラリ
 */

class MCPClient {
    constructor(serverUrl = 'http://127.0.0.1:8081/mcp') {
        this.serverUrl = serverUrl;
        this.requestId = 1;
        this.isConnected = false;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 3;
    }

    /**
     * MCP サーバーへのリクエスト送信
     */
    async makeRequest(method, params = {}) {
        const request = {
            jsonrpc: '2.0',
            method: method,
            params: params,
            id: this.requestId++
        };

        const response = await fetch(this.serverUrl, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(request)
        });

        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }

        const data = await response.json();
        if (data.error) {
            throw new Error(data.error.message || '不明なエラー');
        }

        return data.result;
    }

    /**
     * 接続テスト
     */
    async testConnection() {
        try {
            await this.makeRequest('initialize', {
                protocolVersion: '2024-11-05',
                capabilities: {
                    roots: { listChanged: false }
                },
                clientInfo: {
                    name: 'MCP-RS Web UI',
                    version: '1.0.0'
                }
            });
            
            this.isConnected = true;
            this.reconnectAttempts = 0;
            return true;
        } catch (error) {
            this.isConnected = false;
            throw error;
        }
    }

    /**
     * 自動再接続
     */
    async reconnect() {
        if (this.reconnectAttempts >= this.maxReconnectAttempts) {
            throw new Error('最大再接続回数に達しました');
        }

        this.reconnectAttempts++;
        await new Promise(resolve => setTimeout(resolve, 1000 * this.reconnectAttempts));
        
        return await this.testConnection();
    }

    /**
     * WordPressリソースの取得
     */
    async getWordPressResource(resource) {
        return await this.makeRequest('resources/read', {
            uri: `wordpress://${resource}`
        });
    }

    /**
     * サーバー情報の取得
     */
    async getServerInfo() {
        return await this.makeRequest('initialize', {
            protocolVersion: '2024-11-05',
            capabilities: {},
            clientInfo: {
                name: 'MCP-RS Web UI',
                version: '1.0.0'
            }
        });
    }
}

class MCPUIComponents {
    /**
     * ステータス表示の更新
     */
    static updateStatus(element, type, message, showLoading = false) {
        if (!element) return;

        element.className = `mcp-status ${type}`;
        element.innerHTML = showLoading 
            ? `<span class="mcp-loading"></span> ${message}`
            : message;
    }

    /**
     * 結果表示
     */
    static showResult(container, title, data, timestamp = null) {
        if (!container) return;

        const ts = timestamp || new Date().toLocaleString('ja-JP');
        const formattedData = typeof data === 'object' ? 
            JSON.stringify(data, null, 2) : 
            data;

        container.innerHTML = `
            <div style="border-bottom: 2px solid #e9ecef; padding-bottom: 15px; margin-bottom: 20px;">
                <h3 style="color: #28a745; margin: 0 0 5px 0;">${title}</h3>
                <small style="color: #6c757d;">取得時刻: ${ts}</small>
            </div>
            <div class="mcp-result json">${formattedData}</div>
        `;
    }

    /**
     * エラー表示
     */
    static showError(container, message, timestamp = null) {
        if (!container) return;

        const ts = timestamp || new Date().toLocaleString('ja-JP');
        
        container.innerHTML = `
            <div style="border-bottom: 2px solid #dc3545; padding-bottom: 15px; margin-bottom: 20px;">
                <h3 style="color: #dc3545; margin: 0 0 5px 0;">❌ エラーが発生しました</h3>
                <small style="color: #6c757d;">${ts}</small>
            </div>
            <div style="background: #f8d7da; padding: 15px; border-radius: 8px; color: #721c24;">
                ${message}
            </div>
            <div style="margin-top: 20px; padding: 15px; background: #fff3cd; border-radius: 8px;">
                <strong>💡 トラブルシューティング:</strong>
                <ul style="margin: 10px 0 0 20px; color: #856404;">
                    <li>mcp-rs.exeが起動していることを確認</li>
                    <li>WordPress設定を確認</li>
                    <li>ネットワーク接続を確認</li>
                    <li>ファイアウォール設定を確認</li>
                </ul>
            </div>
        `;
    }

    /**
     * ローディング表示
     */
    static showLoading(container, message) {
        if (!container) return;

        container.innerHTML = `
            <div style="text-align: center; color: #6c757d; padding: 40px;">
                <div class="mcp-loading" style="margin: 0 auto 15px; width: 40px; height: 40px;"></div>
                <p style="font-size: 1.1em;">${message}</p>
            </div>
        `;
    }

    /**
     * ボタン状態の切り替え
     */
    static toggleButton(button, enabled, text = null) {
        if (!button) return;

        button.disabled = !enabled;
        if (text) {
            button.textContent = text;
        }
    }

    /**
     * ナビゲーションのアクティブ状態設定
     */
    static setActiveNav(activeId) {
        document.querySelectorAll('.mcp-nav a').forEach(link => {
            link.classList.remove('active');
        });

        const activeLink = document.getElementById(activeId);
        if (activeLink) {
            activeLink.classList.add('active');
        }
    }

    /**
     * モーダルダイアログ表示
     */
    static showModal(title, content, buttons = []) {
        // モーダル要素の作成
        const modal = document.createElement('div');
        modal.style.cssText = `
            position: fixed;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            background: rgba(0, 0, 0, 0.5);
            display: flex;
            align-items: center;
            justify-content: center;
            z-index: 10000;
        `;

        const modalContent = document.createElement('div');
        modalContent.style.cssText = `
            background: white;
            padding: 30px;
            border-radius: 10px;
            max-width: 500px;
            width: 90%;
            max-height: 80vh;
            overflow-y: auto;
        `;

        modalContent.innerHTML = `
            <h3 style="margin-bottom: 20px; color: #333;">${title}</h3>
            <div style="margin-bottom: 20px;">${content}</div>
            <div style="text-align: right;">
                ${buttons.map(btn => `
                    <button class="mcp-btn ${btn.type || 'secondary'}" 
                            onclick="${btn.onclick || 'this.closest(\'.modal\').remove()'}">
                        ${btn.text}
                    </button>
                `).join(' ')}
            </div>
        `;

        modal.appendChild(modalContent);
        modal.className = 'modal';
        document.body.appendChild(modal);

        // 背景クリックで閉じる
        modal.addEventListener('click', (e) => {
            if (e.target === modal) {
                modal.remove();
            }
        });

        return modal;
    }

    /**
     * ファイルダウンロード
     */
    static downloadFile(content, filename, mimeType = 'text/plain') {
        const blob = new Blob([content], { type: `${mimeType};charset=utf-8` });
        const link = document.createElement('a');
        link.href = URL.createObjectURL(blob);
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(link.href);
    }

    /**
     * クリップボードにコピー
     */
    static async copyToClipboard(text) {
        try {
            await navigator.clipboard.writeText(text);
            return true;
        } catch (err) {
            console.error('Failed to copy to clipboard:', err);
            return false;
        }
    }
}

class MCPConfigManager {
    constructor(mcpClient) {
        this.client = mcpClient;
    }

    /**
     * 現在の設定を取得
     */
    async getCurrentConfig() {
        try {
            return await this.client.makeRequest('config/get');
        } catch (error) {
            console.warn('Config API not available:', error.message);
            return null;
        }
    }

    /**
     * 設定を更新
     */
    async updateConfig(config) {
        return await this.client.makeRequest('config/update', config);
    }

    /**
     * Transport を切り替え
     */
    async switchTransport(transportType, options = {}) {
        return await this.client.makeRequest('transport/switch', {
            type: transportType,
            options: options
        });
    }

    /**
     * 設定をリロード
     */
    async reloadConfig() {
        return await this.client.makeRequest('config/reload');
    }
}

// グローバルユーティリティ
window.MCPUtils = {
    /**
     * URL パラメータの取得
     */
    getUrlParam(name) {
        const urlParams = new URLSearchParams(window.location.search);
        return urlParams.get(name);
    },

    /**
     * 日付フォーマット
     */
    formatDate(date, format = 'datetime') {
        const d = new Date(date);
        switch (format) {
            case 'date':
                return d.toLocaleDateString('ja-JP');
            case 'time':
                return d.toLocaleTimeString('ja-JP');
            default:
                return d.toLocaleString('ja-JP');
        }
    },

    /**
     * サイズフォーマット
     */
    formatSize(bytes) {
        const units = ['B', 'KB', 'MB', 'GB'];
        let size = bytes;
        let unitIndex = 0;

        while (size >= 1024 && unitIndex < units.length - 1) {
            size /= 1024;
            unitIndex++;
        }

        return `${size.toFixed(2)} ${units[unitIndex]}`;
    },

    /**
     * デバウンス
     */
    debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    }
};

// エクスポート
window.MCPClient = MCPClient;
window.MCPUIComponents = MCPUIComponents;
window.MCPConfigManager = MCPConfigManager;