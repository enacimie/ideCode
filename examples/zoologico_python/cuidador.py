from animal import Animal


class Cuidador:
    def __init__(self, nombre: str) -> None:
        self.nombre = nombre
        self.animales: list[Animal] = []

    def adoptar(self, animal: Animal) -> None:
        self.animales.append(animal)

    def ronda(self) -> None:
        for animal in self.animales:
            print(f"{self.nombre} cuida de {animal.presentarse()}")
