import { readFileSync, writeFileSync, readdirSync } from 'fs';
import { join } from 'path';

const distDir = './dist';

function fixPaths(html) {
    // Replace absolute paths with relative paths
    // /_astro/... -> ./_astro/...
    // /styles/... -> ./styles/...
    html = html.replace(/src="\/_astro\//g, 'src="./_astro/');
    html = html.replace(/href="\/_astro\//g, 'href="./_astro/');
    html = html.replace(/src="\//g, 'src="./');
    html = html.replace(/href="\//g, 'href="./');
    return html;
}

// Fix index.html
const indexPath = join(distDir, 'index.html');
let html = readFileSync(indexPath, 'utf-8');
html = fixPaths(html);
writeFileSync(indexPath, html);
console.log('Fixed paths in index.html');

// Also fix any CSS files that reference assets
const cssDir = join(distDir, '_astro');
try {
    const files = readdirSync(cssDir);
    for (const file of files) {
        if (file.endsWith('.css')) {
            const cssPath = join(cssDir, file);
            let css = readFileSync(cssPath, 'utf-8');
            css = css.replace(/url\(\//g, 'url(./');
            writeFileSync(cssPath, css);
            console.log('Fixed paths in', file);
        }
    }
} catch (e) {
    // No CSS files in _astro
}
