import path from 'path';
import fs from 'fs';
import processSvg from './process-svg';
import SVG from './svg';
import { Options } from './types/options';

function optimizeFile(file: string, options: Options) {
  return new Promise((resolve, reject) => {
    const filePath = path.resolve(file);
    fs.readFile(filePath, (error, svgStr) => {
      if (error) reject(new Error(`Could not read one of the svg files: ${filePath}`));

      const svgInstance = new SVG(svgStr.toString());
      const optimizedSvg = processSvg(svgInstance, options);
      fs.writeFile(filePath, optimizedSvg.toString(), (error) => {
        if (error) reject(new Error(`Could not save the optimized svg file: ${filePath}`));
        resolve(true);
      });
    })
  });
}

export default optimizeFile;
