export const onCheckIn = async (event) => {
  console.log(event);
  console.log(event.kind);
  console.log(event.name);

  const result = await fetch("http://localhost:8081/", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      content: `Team ${event.name} just checked in`,
    }),
  });
  console.log(result.status);
  console.log(await result.text());
};

export const onAnnouncement = (event) => {
  console.log(
    `Announcer ${event.announcer} has made the announcement "${event.announcement}"`,
  );
  console.log(`time: ${event.time}`);
};

export const onPause = async (event) => {
  console.log(event);
  console.log(event.kind);
  console.log(event.pausedBy);
  console.log(event.time);

  const result = await fetch("http://localhost:8081/", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      content: `Team ${event.pausedBy} just paused the game bruh`,
    }),
  });
  console.log(result.status);
  console.log(await result.text());
};
