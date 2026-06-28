import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
function runSearch() {
    console.log('TODO');
}
export default function App() {
    return (_jsxs("div", { style: {
            minHeight: '100vh',
            background: '#0a0a0f',
            color: '#f5f5f7',
            fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, sans-serif',
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            gap: '1rem',
        }, children: [_jsx("h1", { style: { margin: 0, fontSize: '2.5rem', fontWeight: 700 }, children: "Balatro Seed Searcher" }), _jsx("p", { style: { margin: 0, fontSize: '1.125rem', opacity: 0.7 }, children: "The fastest Balatro seed search engine on the web." }), _jsx("button", { onClick: runSearch, style: {
                    marginTop: '1rem',
                    padding: '0.75rem 2rem',
                    fontSize: '1rem',
                    fontWeight: 600,
                    background: '#e63946',
                    color: '#f5f5f7',
                    border: 'none',
                    borderRadius: '0.5rem',
                    cursor: 'pointer',
                }, children: "Run search" })] }));
}
