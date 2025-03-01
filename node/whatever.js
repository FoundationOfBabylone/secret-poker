import * as fs from "fs";

let jsonRes;

const path = "response.json";
if (fs.existsSync(path)) {
  const data = fs.readFileSync(path, "utf8");
  jsonRes = JSON.parse(data);
} else {
  console.error("Contract info file not found:", path);
}



let parsedResults = [];

for (let log of jsonRes.jsonLog) {
  if (log.events) {
    for (let event of log.events) {
      if (event.type === "wasm") 
      {
        for (let attribute of event.attributes) {
            if (attribute.key === "response") {
                const parsedRes = attribute.value;
                parsedResults.push(parsedRes);
                break;
            }
        }
      }
    }
  }
}

// console.log(parsedResults);

class A {
  a;
  b;
  constructor(a, b) {
    this.a = a;
    this.b = b;
  }

  // equals(other) {
  //   return this.a === other.a && this.b === other.b;
  // }
}