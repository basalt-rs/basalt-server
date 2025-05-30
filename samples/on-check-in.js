export const handle = async (event) => {
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
