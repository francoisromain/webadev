(function () {
  var button = document.getElementById("load-commits");
  var list = document.getElementById("commit-list");

  if (!button || !list) {
    return;
  }

  function formatDate(iso) {
    var d = new Date(iso);
    if (isNaN(d.getTime())) {
      return iso;
    }
    return d.toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  }

  function render(commits) {
    list.textContent = "";
    commits.forEach(function (commit) {
      var li = document.createElement("li");

      var hash = document.createElement("code");
      hash.className = "commit-hash";
      hash.textContent = commit.sha.slice(0, 7);

      var date = document.createElement("span");
      date.className = "commit-date";
      date.textContent = formatDate(commit.commit.author.date);

      var msg = document.createElement("span");
      msg.className = "commit-message";
      msg.textContent = commit.commit.message.split("\n")[0];

      li.appendChild(hash);
      li.appendChild(document.createTextNode(" "));
      li.appendChild(date);
      li.appendChild(document.createTextNode(" "));
      li.appendChild(msg);
      list.appendChild(li);
    });
  }

  function fail(msg) {
    list.textContent = "";
    var li = document.createElement("li");
    li.className = "commit-error";
    li.textContent = msg;
    list.appendChild(li);
  }

  button.addEventListener("click", function () {
    fetch("https://api.github.com/repos/francoisromain/webadev/commits?per_page=10")
      .then(function (res) {
        if (!res.ok) {
          throw new Error("HTTP " + res.status);
        }
        return res.json();
      })
      .then(render)
      .catch(function (err) {
        console.error("Failed to load commits:", err);
        fail("Could not load commits. Is the server reachable and CORS allowed?");
      });
  });
})();
