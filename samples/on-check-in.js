export const handle = async (event) => {
  console.log(event);
  console.log(event.kind);
  console.log(event.name);

  await fetch("http://localhost:8081/", {
    method: "POST",
    body: JSON.stringify({
      content: `Team ${event.name} just checked in`,
    }),
  });
};
