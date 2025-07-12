const App = () => {
  const handleClick = () => {
    alert("🎉 Hello from React + Rust Dev Server!");
  };

  return (
    <main role="main" aria-label="Main content area" className="container">
      <header>
        <h1 className="fade-in">⚙️ Welcome to Rust Dev Server</h1>
        <p className="subtext">
          Edit any file in <code>/client</code> to see live updates instantly!
        </p>
      </header>

      <section className="tinker-box" aria-labelledby="editBoxLabel">
        <h2 id="editBoxLabel">🧪 Try editing this section</h2>
        <p>
          This area is hot-reloadable. Change this text, save, and watch it reload automatically.
        </p>
        <button className="btn" aria-label="Click me for fun" onClick={handleClick}>
          Click Me
        </button>
      </section>

      <footer>
        <p>Built with 💻 + 🦀 — Happy Hacking!</p>
      </footer>
    </main>
  );
};

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
