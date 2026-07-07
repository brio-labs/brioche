import { useMemo } from 'react';

interface MarkdownRendererProps {
    content: string;
}

// Simple markdown parser - no external dependencies
export default function MarkdownRenderer({ content }: MarkdownRendererProps) {
    const html = useMemo(() => {
        return parseMarkdown(content);
    }, [content]);

    return (
        <div
            className="message-content"
            dangerouslySetInnerHTML={{ __html: html }}
        />
    );
}

function parseMarkdown(text: string): string {
    let html = escapeHtml(text);

    // Code blocks (fenced)
    html = html.replace(
        /```(\w+)?\n([\s\S]*?)```/g,
        (_match, lang, code) => {
            const language = lang || 'text';
            const escapedCode = escapeHtml(code.trimEnd());
            return `<div class="code-block-wrapper">
                <div class="code-block-header">
                    <span class="lang-label">${language}</span>
                    <button class="copy-btn" onclick="navigator.clipboard.writeText(this.parentElement.nextElementSibling.textContent).then(()=>{this.textContent='Copied!';setTimeout(()=>this.textContent='Copy',1500)})">Copy</button>
                </div>
                <pre><code>${escapedCode}</code></pre>
            </div>`;
        }
    );

    // Inline code
    html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

    // Headers
    html = html.replace(/^### (.*$)/gim, '<h3>$1</h3>');
    html = html.replace(/^## (.*$)/gim, '<h2>$1</h2>');
    html = html.replace(/^# (.*$)/gim, '<h1>$1</h1>');

    // Bold and italic
    html = html.replace(/\*\*\*(.*?)\*\*\*/g, '<strong><em>$1</em></strong>');
    html = html.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/\*(.*?)\*/g, '<em>$1</em>');
    html = html.replace(/__(.*?)__/g, '<strong>$1</strong>');
    html = html.replace(/_(.*?)_/g, '<em>$1</em>');

    // Blockquotes
    html = html.replace(/^\> (.*$)/gim, '<blockquote>$1</blockquote>');

    // Links
    html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');

    // Unordered lists
    html = html.replace(/^(\s*)[-*+] (.*$)/gim, '<li>$2</li>');
    html = html.replace(/(<li>.*<\/li>\n?)+/g, '<ul>$&</ul>');
    html = html.replace(/<\/ul>\s*<ul>/g, '');

    // Ordered lists
    html = html.replace(/^(\s*)\d+\.\s+(.*$)/gim, '<li>$2</li>');

    // Horizontal rules
    html = html.replace(/^---$/gim, '<hr>');
    html = html.replace(/^\*\*\*$/gim, '<hr>');

    // Line breaks - convert remaining newlines to <br> or <p>
    const paragraphs = html.split('\n\n').map(p => {
        const trimmed = p.trim();
        if (!trimmed) return '';
        if (trimmed.startsWith('<')) return trimmed;
        return `<p>${trimmed.replace(/\n/g, '<br>')}</p>`;
    });

    return paragraphs.join('\n');
}

function escapeHtml(text: string): string {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
}
