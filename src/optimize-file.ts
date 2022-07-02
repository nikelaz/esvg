import path from 'path';
import fs from 'fs';
import processSvg from './process-svg';
import SVG from './svg';

function optimizeFile(file: string, pretty: Boolean) {
  return new Promise((resolve, reject) => {
    const filePath = path.resolve(file);
    fs.readFile(filePath, (error, svgStr) => {
      if (error) reject(new Error(`Could not read one of the svg files: ${filePath}`));

      const svgInstance = new SVG(svgStr.toString());
      const optimizedSvg = processSvg(svgInstance, pretty);
      fs.writeFile(filePath, optimizedSvg.toString(), (error) => {
        if (error) reject(new Error(`Could not save the optimized svg file: ${filePath}`));
        resolve(true);
      });
    })
  });
}

export default optimizeFile;
